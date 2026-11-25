// Only commands and resource bounds are generated above. Context stays in memory.
export default {
  id: configuration.identifier,
  async setup(ctx) {
    const directory = ctx.location?.directory;
    if (typeof directory !== "string" || directory.length === 0) {
      throw new Error("Graphit plugin directory unavailable");
    }
    const running = new Set();
    const registrations = [];
    const subscription = new AbortController();
    let disposed = false;
    let streamFailure;
    function failure(source) {
      const error = new Error("Graphit hook invocation failed");
      // Retain causal status without copying commands, private output or SDK payloads.
      error.code = source?.code ?? source?.name;
      error.signal = source?.signal;
      error.killed = source?.killed;
      return error;
    }
    function invoke(command, event, sessionID, consume, location) {
      if (disposed || streamFailure) return Promise.reject(streamFailure ?? failure({ code: "ABORT_ERR" }));
      if (typeof sessionID !== "string" || sessionID.length === 0) {
        return Promise.reject(failure({ code: "INVALID_EVENT" }));
      }
      const controller = new AbortController();
      const expires = performance.now() + configuration.timeout;
      const record = { sessionID, task: undefined, child: undefined, closed: undefined, cause: undefined };
      record.stop = (cause = { code: "ABORT_ERR" }) => {
        record.cause ??= failure(cause);
        // Node's AbortSignal may send SIGTERM regardless of execFile.killSignal.
        record.child?.kill("SIGKILL");
        controller.abort(record.cause);
      };
      const timer = setTimeout(() => record.stop({ code: "ETIMEDOUT" }), configuration.timeout);
      running.add(record);
      record.task = (async () => {
        let actual = location;
        if (actual === undefined) {
          // The hook's session can belong to a different location than this plugin instance.
          const session = await ctx.session.get({ sessionID }, { signal: controller.signal });
          actual = session.location?.directory;
        }
        if (controller.signal.aborted) throw record.cause;
        if (typeof actual !== "string" || resolve(actual) !== resolve(directory)) {
          throw failure({ code: "LOCATION_UNAVAILABLE" });
        }
        const input = JSON.stringify({ cwd: actual, hook_event_name: event });
        if (Buffer.byteLength(input) > configuration.max_input_bytes) throw failure({ code: "INPUT_LIMIT" });
        return await new Promise((resolveOutput, reject) => {
          const remaining = Math.ceil(expires - performance.now());
          if (remaining <= 0) { record.stop({ code: "ETIMEDOUT" }); reject(record.cause); return; }
          const child = execFile(command.program, command.args, {
            cwd: actual,
            encoding: "buffer",
            timeout: remaining,
            maxBuffer: configuration.max_output_bytes,
            signal: controller.signal,
            killSignal: "SIGKILL",
            windowsHide: true,
          }, (error, stdout) => {
            if (record.cause || error || disposed) {
              reject(record.cause ?? failure(error ?? { code: "ABORT_ERR" }));
              return;
            }
            let text;
            try { text = new TextDecoder("utf-8", { fatal: true }).decode(stdout); }
            catch { reject(failure({ code: "INVALID_ENCODING" })); return; }
            // Consume synchronously while cancellation still owns this invocation.
            try { resolveOutput(consume ? consume(text) : text); }
            catch (error) { reject(error); }
          });
          record.child = child;
          record.closed = new Promise((closed) => child.once("close", closed));
          if (child.stdin) {
            child.stdin.on("error", (error) => {
              record.cause ??= failure(error);
              reject(record.cause);
              record.stop(error);
            });
            child.stdin.end(input);
          }
        });
      })().catch((error) => { throw record.cause ?? failure(error); }).finally(async () => {
        clearTimeout(timer);
        await record.closed;
        running.delete(record);
      });
      return record.task;
    }
    async function cancel(sessionID, all = false) {
      const active = [...running].filter((record) => all || record.sessionID === sessionID);
      for (const record of active) record.stop();
      await Promise.allSettled(active.map((record) => record.task));
    }
    const context = (command, event) => async (input) => {
      if (!Array.isArray(input.system)) throw failure({ code: "INVALID_OUTPUT" });
      if (event === "compaction" && input.result !== undefined) throw failure({ code: "CONTEXT_UNAVAILABLE" });
      await invoke(command, event, input.sessionID, (text) => {
        if (text.length === 0) throw failure({ code: "EMPTY_CONTEXT" });
        input.system.push({ type: "text", text });
      });
    };
    async function cleanup() {
      disposed = true;
      subscription.abort();
      await cancel(undefined, true);
      await events;
      await Promise.all(registrations.map((registration) => registration.dispose()));
      if (streamFailure) throw streamFailure;
    }
    let events;
    try {
      registrations.push(await ctx.session.hook("context", context(configuration.context, "context")));
      registrations.push(await ctx.session.hook("compaction", context(configuration.compaction, "compaction")));
      const stream = await ctx.event.subscribe({ signal: subscription.signal });
      events = (async () => {
        for await (const event of stream) {
          if (disposed) break;
          if (!["session.deleted", "session.idle", "session.execution.interrupted"].includes(event.type)) continue;
          const sessionID = event.data?.sessionID;
          if (typeof sessionID !== "string") throw failure({ code: "INVALID_EVENT" });
          if (event.type === "session.execution.interrupted" || event.type === "session.deleted") await cancel(sessionID);
          const command = event.type === "session.deleted" ? configuration.deleted : event.type === "session.idle" ? configuration.idle : undefined;
          if (command) {
            const location = event.location?.directory;
            // Subscription includes every server location. Never execute another project's hooks.
            if (typeof location !== "string") throw failure({ code: "LOCATION_UNAVAILABLE" });
            if (resolve(location) === resolve(directory)) {
              void invoke(command, event.type, sessionID, undefined, location).catch(async (error) => {
                if (!disposed && error.code !== "ABORT_ERR") { streamFailure ??= failure(error); await cancel(undefined, true); }
              });
            }
          }
        }
      })().catch(async (error) => {
        if (!disposed) { streamFailure ??= failure(error); await cancel(undefined, true); }
      });
    } catch (error) {
      await cleanup();
      throw failure(error);
    }
    return cleanup;
  },
};
