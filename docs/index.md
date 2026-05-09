# v8-python

`v8-python` provides Python bindings for embedding V8 and running JavaScript,
built on `denoland/rusty_v8`.

Use it to create V8 isolates and contexts, evaluate JavaScript, exchange values
between Python and JavaScript, expose Python functions and classes, and install
host APIs such as timers, console, module loading, and WebAssembly. The runtime
binding layer is implemented in Rust using `denoland/rusty_v8`.

## Install

```bash
pip install v8-python
```

if you're using uv, you can also install with:

```bash
uv add v8-python
```

For local development:

```bash
uv run maturin develop
```

## Documentation

- Start with the [tutorial](tutorial.md) for basic usage.
- See the [API reference](api-reference/index.md) for the generated Python API surface.
