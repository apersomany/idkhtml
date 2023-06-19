import Context from "https://deno.land/std@0.192.0/wasi/snapshot_preview1.ts";

const mod = await WebAssembly.compile(await Deno.readFile("target/wasm32-wasi/release/idkhtml.wasm"));
// warm up executions
// for (let i = 0; i < 4; i++) {
//     // a bit hacky, but redirect stdout to stdin to hide output
//     const ctx = new Context({ stdout: 3 });
//     const ins = new WebAssembly.Instance(mod, { wasi_snapshot_preview1: ctx.exports });
//     ctx.start(ins);
// }
// actual execution
const ctx = new Context();
const ins = new WebAssembly.Instance(mod, { wasi_snapshot_preview1: ctx.exports });
ctx.start(ins);
