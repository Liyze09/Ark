use wasmtime::component::bindgen;

bindgen!({
    world: "entry",
    anyhow: true,
});
