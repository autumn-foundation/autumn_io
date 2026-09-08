// A minimal `wasi_snapshot_preview1` host for an Autumn edge capsule.
//
// This is the JavaScript counterpart of `autumn_edge::host`, and it is
// deliberately just as small. A capsule's only way out is the stdio dialogue,
// so the syscalls it is given are exactly the ones that dialogue needs:
//
//   fd_read / fd_write   the wire itself
//   environ_* / args_*   what the Rust runtime touches during startup
//   random_get           HashMap seeds
//   proc_exit            the end of `main`
//
// Everything else is stubbed to an errno. There is no filesystem: no
// `path_open`, no resolving `fd_prestat_*`, and no sockets. `assertImports` in
// `capsule.js` checks a built artifact against that list, so a dependency that
// starts reaching for the network or the disk fails the build rather than
// silently getting a stub that lies to it.
//
// The clock is pinned to zero and randomness is deterministic, matching the
// reference host. That does not make a non-deterministic handler correct — a
// real CDN host promises neither — but it does mean this shim never becomes the
// reason two lanes disagree.

/** WASI errno values this shim returns. */
const ERRNO = {
  SUCCESS: 0,
  BADF: 8,
  INVAL: 28,
  NOSPC: 51,
  NOTSUP: 58,
  SPIPE: 70,
};

const STDIN = 0;
const STDOUT = 1;
const STDERR = 2;

/** One WASI `iovec`: a `u32` pointer and a `u32` length. */
const IOVEC_SIZE = 8;

/** Size of a WASI `fdstat` struct. */
const FDSTAT_SIZE = 24;

/** WASI filetype for a character device, which is what a pipe presents as. */
const FILETYPE_CHARACTER_DEVICE = 2;

/**
 * Thrown by `proc_exit` to unwind out of the guest.
 *
 * WASI has no way to return from `_start` with a code, so the exit is a
 * non-local jump. The driver catches this and reads the status off it.
 */
export class ProcExit extends Error {
  /** @param {number} code */
  constructor(code) {
    super(`capsule called proc_exit(${code})`);
    this.name = "ProcExit";
    this.code = code;
  }
}

/**
 * A WASI host bound to one guest instance.
 *
 * Create it, pass `imports` to `WebAssembly.instantiate`, then hand the
 * instance back with `bind` — the memory is not known until instantiation, so
 * the two steps cannot be one.
 */
export class Wasi {
  /**
   * @param {object} options
   * @param {(line: string) => (string | null)} options.onStdoutLine
   *   Called with each complete line the guest writes to stdout, newline
   *   stripped. Return a line to push onto the guest's stdin (the answer to a
   *   `kv_get`), or `null` for nothing. This is what makes a mid-request KV
   *   round trip work without the host ever suspending.
   * @param {number} [options.stdoutLineBudget]
   *   Cap on a single newline-less stdout line, in bytes. A guest streaming
   *   without ever ending a line would otherwise grow a host buffer without
   *   limit. Mirrors `autumn_edge::host::STDOUT_LINE_BUDGET_BYTES`.
   */
  constructor({ onStdoutLine, stdoutLineBudget = 8 * 1024 * 1024 }) {
    this.onStdoutLine = onStdoutLine;
    this.stdoutLineBudget = stdoutLineBudget;
    /** @type {WebAssembly.Memory | null} */
    this.memory = null;
    /** Bytes the guest has not read yet. */
    this.stdin = new Uint8Array(0);
    /** Whether stdin has been closed; a read past this is EOF. */
    this.stdinClosed = false;
    /** Partial stdout line, awaiting its newline. */
    this.stdoutPending = "";
    /** Everything the guest wrote to stderr, for diagnostics. */
    this.stderr = "";
    this.decoder = new TextDecoder();
    this.encoder = new TextEncoder();
    // A tiny deterministic PRNG. `random_get` must return *something* — Rust's
    // std seeds its hasher from it — but nothing at the edge may depend on the
    // values, so a fixed sequence is the honest answer.
    this.randomState = 0x9e3779b9;
  }

  /** @param {WebAssembly.Instance} instance */
  bind(instance) {
    this.memory = instance.exports.memory;
  }

  /** Append bytes to the guest's stdin. */
  pushStdin(text) {
    const bytes = this.encoder.encode(text);
    const merged = new Uint8Array(this.stdin.length + bytes.length);
    merged.set(this.stdin);
    merged.set(bytes, this.stdin.length);
    this.stdin = merged;
  }

  /** No more input is coming; the guest's next read returns EOF. */
  closeStdin() {
    this.stdinClosed = true;
  }

  /**
   * A fresh view of guest memory.
   *
   * Re-created per access rather than cached: `memory.grow` detaches the old
   * `ArrayBuffer`, and a stale view throws on every subsequent read.
   */
  get view() {
    return new DataView(this.memory.buffer);
  }

  get bytes() {
    return new Uint8Array(this.memory.buffer);
  }

  /** The import object to instantiate the guest with. */
  get imports() {
    const self = this;
    return {
      wasi_snapshot_preview1: {
        args_sizes_get(argcPtr, argvBufSizePtr) {
          const view = self.view;
          view.setUint32(argcPtr, 0, true);
          view.setUint32(argvBufSizePtr, 0, true);
          return ERRNO.SUCCESS;
        },
        args_get() {
          return ERRNO.SUCCESS;
        },
        environ_sizes_get(countPtr, sizePtr) {
          const view = self.view;
          view.setUint32(countPtr, 0, true);
          view.setUint32(sizePtr, 0, true);
          return ERRNO.SUCCESS;
        },
        environ_get() {
          return ERRNO.SUCCESS;
        },
        clock_res_get(_id, resultPtr) {
          self.view.setBigUint64(resultPtr, 1n, true);
          return ERRNO.SUCCESS;
        },
        // Pinned, like the reference host. A handler that reads the clock is
        // already outside the byte-identity contract; this at least makes the
        // breach reproducible instead of intermittent.
        clock_time_get(_id, _precision, resultPtr) {
          self.view.setBigUint64(resultPtr, 0n, true);
          return ERRNO.SUCCESS;
        },
        fd_close(fd) {
          return fd === STDIN || fd === STDOUT || fd === STDERR
            ? ERRNO.SUCCESS
            : ERRNO.BADF;
        },
        fd_fdstat_get(fd, resultPtr) {
          if (fd !== STDIN && fd !== STDOUT && fd !== STDERR) return ERRNO.BADF;
          const bytes = self.bytes;
          bytes.fill(0, resultPtr, resultPtr + FDSTAT_SIZE);
          self.view.setUint8(resultPtr, FILETYPE_CHARACTER_DEVICE);
          return ERRNO.SUCCESS;
        },
        fd_fdstat_set_flags() {
          return ERRNO.SUCCESS;
        },
        // No preopens: there is no filesystem to open anything in. `BADF` is
        // what the Rust runtime's preopen scan expects as "that is the end of
        // the list".
        fd_prestat_get() {
          return ERRNO.BADF;
        },
        fd_prestat_dir_name() {
          return ERRNO.BADF;
        },
        fd_read(fd, iovsPtr, iovsLen, nreadPtr) {
          if (fd !== STDIN) return ERRNO.BADF;
          const view = self.view;
          const bytes = self.bytes;
          let read = 0;
          for (let i = 0; i < iovsLen && self.stdin.length > 0; i += 1) {
            const base = view.getUint32(iovsPtr + i * IOVEC_SIZE, true);
            const len = view.getUint32(iovsPtr + i * IOVEC_SIZE + 4, true);
            const take = Math.min(len, self.stdin.length);
            if (take === 0) continue;
            bytes.set(self.stdin.subarray(0, take), base);
            self.stdin = self.stdin.subarray(take);
            read += take;
          }
          // An empty read is EOF, which ends the guest's serve loop. Since the
          // host answers every `kv_get` inline before the guest gets here,
          // there is never a legitimate reason to block.
          view.setUint32(nreadPtr, read, true);
          return ERRNO.SUCCESS;
        },
        fd_write(fd, iovsPtr, iovsLen, nwrittenPtr) {
          if (fd !== STDOUT && fd !== STDERR) return ERRNO.BADF;
          const view = self.view;
          const bytes = self.bytes;
          let written = 0;
          let chunk = "";
          for (let i = 0; i < iovsLen; i += 1) {
            const base = view.getUint32(iovsPtr + i * IOVEC_SIZE, true);
            const len = view.getUint32(iovsPtr + i * IOVEC_SIZE + 4, true);
            chunk += self.decoder.decode(bytes.subarray(base, base + len), {
              stream: true,
            });
            written += len;
          }
          if (fd === STDERR) {
            self.stderr += chunk;
            view.setUint32(nwrittenPtr, written, true);
            return ERRNO.SUCCESS;
          }
          const errno = self.consumeStdout(chunk);
          if (errno !== ERRNO.SUCCESS) return errno;
          view.setUint32(nwrittenPtr, written, true);
          return ERRNO.SUCCESS;
        },
        // stdio is a pipe.
        fd_seek() {
          return ERRNO.SPIPE;
        },
        fd_filestat_get() {
          return ERRNO.BADF;
        },
        fd_tell() {
          return ERRNO.SPIPE;
        },
        fd_sync() {
          return ERRNO.SUCCESS;
        },
        fd_datasync() {
          return ERRNO.SUCCESS;
        },
        path_open() {
          return ERRNO.NOTSUP;
        },
        path_filestat_get() {
          return ERRNO.NOTSUP;
        },
        poll_oneoff() {
          return ERRNO.NOTSUP;
        },
        proc_exit(code) {
          throw new ProcExit(code);
        },
        random_get(ptr, len) {
          const bytes = self.bytes;
          for (let i = 0; i < len; i += 1) {
            // xorshift32, so the sequence is fixed and cheap.
            self.randomState ^= self.randomState << 13;
            self.randomState ^= self.randomState >>> 17;
            self.randomState ^= self.randomState << 5;
            bytes[ptr + i] = self.randomState & 0xff;
          }
          return ERRNO.SUCCESS;
        },
        sched_yield() {
          return ERRNO.SUCCESS;
        },
      },
    };
  }

  /**
   * Accumulate stdout and hand each complete line to the driver, pushing
   * whatever it answers with straight back onto stdin.
   *
   * @param {string} chunk
   * @returns {number} a WASI errno
   */
  consumeStdout(chunk) {
    this.stdoutPending += chunk;
    let newline = this.stdoutPending.indexOf("\n");
    while (newline !== -1) {
      const line = this.stdoutPending.slice(0, newline);
      this.stdoutPending = this.stdoutPending.slice(newline + 1);
      const reply = this.onStdoutLine(line);
      if (reply !== null && reply !== undefined) this.pushStdin(reply);
      newline = this.stdoutPending.indexOf("\n");
    }
    if (this.stdoutPending.length > this.stdoutLineBudget) {
      // A line that never ends. Failing the write is what turns this into a
      // `capsule_error` fallthrough instead of an unbounded host allocation.
      return ERRNO.NOSPC;
    }
    return ERRNO.SUCCESS;
  }
}

/** Syscalls this shim implements. Nothing else may appear in an artifact. */
export const ALLOWED_IMPORTS = Object.freeze([
  "args_get",
  "args_sizes_get",
  "clock_res_get",
  "clock_time_get",
  "environ_get",
  "environ_sizes_get",
  "fd_close",
  "fd_datasync",
  "fd_fdstat_get",
  "fd_fdstat_set_flags",
  "fd_filestat_get",
  "fd_prestat_dir_name",
  "fd_prestat_get",
  "fd_read",
  "fd_seek",
  "fd_sync",
  "fd_tell",
  "fd_write",
  "path_filestat_get",
  "path_open",
  "poll_oneoff",
  "proc_exit",
  "random_get",
  "sched_yield",
]);
