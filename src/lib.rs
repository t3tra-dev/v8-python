use pyo3::prelude::{
    Bound, PyAny, PyModule, PyResult, Python, pyfunction, pymodule, wrap_pyfunction,
};
use pyo3::types::{PyAnyMethods, PyList, PyListMethods, PyModuleMethods};
use pyo3_stub_gen::define_stub_info_gatherer;
use pyo3_stub_gen::derive::gen_stub_pyfunction;

pyo3_stub_gen::derive::gen_type_alias_from_python!(
    "v8",
    r#"
    from typing import TypeAlias
    import collections.abc

    _JSBytesLike: TypeAlias = bytes | bytearray | memoryview[int]
    """Python bytes-like objects accepted by V8 conversion APIs."""

    _JSArrayLike: TypeAlias = list[object] | tuple[object, ...]
    """Python sequence containers converted to JavaScript arrays."""

    _JSObjectLike: TypeAlias = dict[object, object]
    """Python mapping container converted to a JavaScript object."""

    _JSValueLike: TypeAlias = Value | String | Object | Array | Function | Promise | BigInt | Symbol | ArrayBuffer | ArrayBufferView | TypedArray | DataView | Map | Set | Date | RegExp | Proxy | External | WasmModule | str | int | float | bool | None | _JSBytesLike | _JSArrayLike | _JSObjectLike
    """Python values accepted by APIs that convert inputs to JavaScript values."""

    _JSFunctionArgsLike: TypeAlias = list[_JSValueLike] | tuple[_JSValueLike, ...]
    """Positional JavaScript call arguments."""

    _JSPropertyNameLike: TypeAlias = str | String | Symbol | Value
    """Values accepted where V8 requires a string or symbol property name."""

    _JSAccessorLike: TypeAlias = Function | Value
    """JavaScript function value accepted as an accessor getter or setter."""

    _HostCallable: TypeAlias = collections.abc.Callable[..., object]
    """Python callable that can be exposed to JavaScript."""

    _HostFunctionDecorator: TypeAlias = collections.abc.Callable[[_HostCallable], _HostCallable]
    """Decorator returned by host_function when no callable is supplied."""

    _HostClassLike: TypeAlias = type[object]
    """Python class that can be exposed as a JavaScript constructor template."""

    _HostClassDecorator: TypeAlias = collections.abc.Callable[[_HostClassLike], _HostClassLike]
    """Decorator returned by class_ when no class is supplied."""

    _ModuleImportsLike: TypeAlias = dict[str, str | Module]
    """Module instantiation imports keyed by module specifier."""
    "#
);

pyo3_stub_gen::derive::gen_type_alias_from_python!(
    "v8.api",
    r#"
    from typing import TypeAlias
    import collections.abc

    _WebAssemblyLoaderLike: TypeAlias = collections.abc.Mapping[str, bytes] | collections.abc.Callable[[str], bytes]
    """Python WebAssembly streaming loader accepted by v8.api.WebAssembly."""
    "#
);

mod isolate;
use isolate::Isolate;

mod error;
use error::{JavaScriptError, JavaScriptMessage, StackFrame, StackTrace};

mod event_loop;

mod runtime;

mod heap;

mod snapshot;
mod structured_clone;
mod templates;

mod host_apis;

mod profile;
use profile::BaseProfile;

mod module;
use module::Module;

use host_apis::inspector::{Inspector, InspectorSession};

mod context;
use context::{Context, ContextBuilder};

mod scope;
use scope::Scope;

mod script;
use script::Script;
use snapshot::{SnapshotCreator, StartupData};

mod v8value;
use v8value::{
    PropertyAttribute, PropertyDescriptor, V8Array, V8ArrayBuffer, V8ArrayBufferView, V8BigInt,
    V8CompiledWasmModule, V8DataView, V8Date, V8External, V8Function, V8Map, V8Object, V8Private,
    V8Promise, V8Proxy, V8RegExp, V8Set, V8String, V8Symbol, V8TypedArray, V8WasmModule, Value,
    WasmModuleCache,
};

/// Run Python's gc.collect(), then release pending V8 isolates in safe reverse creation order.
#[gen_stub_pyfunction]
#[pyfunction]
fn collect_garbage(py: Python<'_>) -> PyResult<usize> {
    let dropped_before = runtime::dropped_isolate_count();

    py.import("gc")?.call_method0("collect")?;
    runtime::collect_ready_isolates();

    Ok(runtime::dropped_isolate_count().saturating_sub(dropped_before))
}

/// Return V8's cached-data version tag for script code cache compatibility checks.
#[gen_stub_pyfunction]
#[pyfunction]
fn cached_data_version_tag() -> u32 {
    runtime::init_v8_once();
    v8::script_compiler::cached_data_version_tag()
}

/// Internal callback installed into Python's gc module to release retired V8 isolates.
#[gen_stub_pyfunction]
#[pyfunction(name = "_gc_callback")]
fn gc_callback(
    phase: &str,
    #[gen_stub(override_type(type_repr = "object", imports = ()))] _info: &Bound<'_, PyAny>,
) -> PyResult<()> {
    if phase == "stop" {
        runtime::try_collect_ready_isolates();
    }

    Ok(())
}

fn install_gc_callback(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let gc = m.py().import("gc")?;
    let callbacks = gc.getattr("callbacks")?.cast_into::<PyList>()?;
    let callback = wrap_pyfunction!(gc_callback, m)?;
    let callback_module = string_attr(callback.as_any(), "__module__")?;
    let callback_name = string_attr(callback.as_any(), "__name__")?;

    for index in (0..callbacks.len()).rev() {
        let callback = callbacks.get_item(index)?;

        if is_installed_gc_callback(&callback, &callback_module, &callback_name) {
            callbacks.del_item(index)?;
        }
    }

    callbacks.append(callback.clone())?;
    m.add_function(callback)?;

    Ok(())
}

fn is_installed_gc_callback(callback: &Bound<'_, PyAny>, module: &str, name: &str) -> bool {
    let Ok(callback_module) = string_attr(callback, "__module__") else {
        return false;
    };
    let Ok(callback_name) = string_attr(callback, "__name__") else {
        return false;
    };

    callback_module == module && callback_name == name
}

fn string_attr(object: &Bound<'_, PyAny>, name: &str) -> PyResult<String> {
    object.getattr(name)?.extract()
}

#[pymodule]
#[pyo3(name = "v8")]
fn v8_python(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Isolate>()?;
    m.add_class::<JavaScriptError>()?;
    m.add_class::<JavaScriptMessage>()?;
    m.add_class::<StackTrace>()?;
    m.add_class::<StackFrame>()?;
    m.add_class::<BaseProfile>()?;
    m.add_class::<StartupData>()?;
    m.add_class::<SnapshotCreator>()?;
    m.add_class::<Inspector>()?;
    m.add_class::<InspectorSession>()?;
    m.add_class::<Context>()?;
    m.add_class::<ContextBuilder>()?;
    m.add_class::<Scope>()?;
    m.add_class::<V8String>()?;
    m.add_class::<V8Object>()?;
    m.add_class::<V8Array>()?;
    m.add_class::<V8Function>()?;
    m.add_class::<V8Promise>()?;
    m.add_class::<V8BigInt>()?;
    m.add_class::<V8Symbol>()?;
    m.add_class::<V8ArrayBuffer>()?;
    m.add_class::<V8ArrayBufferView>()?;
    m.add_class::<V8TypedArray>()?;
    m.add_class::<V8DataView>()?;
    m.add_class::<V8Map>()?;
    m.add_class::<V8Set>()?;
    m.add_class::<V8Date>()?;
    m.add_class::<V8RegExp>()?;
    m.add_class::<V8Proxy>()?;
    m.add_class::<V8Private>()?;
    m.add_class::<V8External>()?;
    m.add_class::<V8WasmModule>()?;
    m.add_class::<V8CompiledWasmModule>()?;
    m.add_class::<WasmModuleCache>()?;
    m.add_class::<PropertyAttribute>()?;
    m.add_class::<PropertyDescriptor>()?;
    m.add_class::<Value>()?;
    m.add_class::<Script>()?;
    m.add_class::<Module>()?;
    m.add_function(wrap_pyfunction!(cached_data_version_tag, m)?)?;
    m.add_function(wrap_pyfunction!(collect_garbage, m)?)?;
    host_apis::install_api_module(m)?;
    install_gc_callback(m)?;
    Ok(())
}

define_stub_info_gatherer!(stub_info);
