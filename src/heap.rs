use pyo3::prelude::{Py, PyAny, PyResult, Python};
use pyo3::types::{PyDict, PyDictMethods, PyList, PyListMethods};

use super::runtime::SharedIsolate;

pub(crate) fn heap_statistics(py: Python<'_>, isolate: &SharedIsolate) -> PyResult<Py<PyAny>> {
    let mut isolate = isolate.borrow_mut();
    let statistics = isolate.get_heap_statistics();
    let result = PyDict::new(py);

    result.set_item("total_heap_size", statistics.total_heap_size())?;
    result.set_item(
        "total_heap_size_executable",
        statistics.total_heap_size_executable(),
    )?;
    result.set_item("total_physical_size", statistics.total_physical_size())?;
    result.set_item("total_available_size", statistics.total_available_size())?;
    result.set_item(
        "total_global_handles_size",
        statistics.total_global_handles_size(),
    )?;
    result.set_item(
        "used_global_handles_size",
        statistics.used_global_handles_size(),
    )?;
    result.set_item("used_heap_size", statistics.used_heap_size())?;
    result.set_item("heap_size_limit", statistics.heap_size_limit())?;
    result.set_item("malloced_memory", statistics.malloced_memory())?;
    result.set_item("external_memory", statistics.external_memory())?;
    result.set_item("peak_malloced_memory", statistics.peak_malloced_memory())?;
    result.set_item(
        "number_of_native_contexts",
        statistics.number_of_native_contexts(),
    )?;
    result.set_item(
        "number_of_detached_contexts",
        statistics.number_of_detached_contexts(),
    )?;
    result.set_item("total_allocated_bytes", statistics.total_allocated_bytes())?;
    result.set_item("does_zap_garbage", statistics.does_zap_garbage())?;

    Ok(result.into_any().unbind())
}

pub(crate) fn heap_space_statistics(
    py: Python<'_>,
    isolate: &SharedIsolate,
) -> PyResult<Py<PyAny>> {
    let mut isolate = isolate.borrow_mut();
    let result = PyList::empty(py);

    for index in 0..isolate.number_of_heap_spaces() {
        let Some(statistics) = isolate.get_heap_space_statistics(index) else {
            continue;
        };
        let space_name = statistics.space_name().to_str().map_err(|_| {
            pyo3::exceptions::PyRuntimeError::new_err("V8 returned an invalid heap space name.")
        })?;
        let space = PyDict::new(py);

        space.set_item("space_name", space_name)?;
        space.set_item("space_size", statistics.space_size())?;
        space.set_item("space_used_size", statistics.space_used_size())?;
        space.set_item("space_available_size", statistics.space_available_size())?;
        space.set_item("physical_space_size", statistics.physical_space_size())?;
        result.append(space)?;
    }

    Ok(result.into_any().unbind())
}

pub(crate) fn heap_code_statistics(py: Python<'_>, isolate: &SharedIsolate) -> PyResult<Py<PyAny>> {
    let mut isolate = isolate.borrow_mut();
    let statistics = isolate
        .get_heap_code_and_metadata_statistics()
        .ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err(
                "V8 failed to return heap code and metadata statistics.",
            )
        })?;
    let result = PyDict::new(py);

    result.set_item(
        "code_and_metadata_size",
        statistics.code_and_metadata_size(),
    )?;
    result.set_item(
        "bytecode_and_metadata_size",
        statistics.bytecode_and_metadata_size(),
    )?;
    result.set_item(
        "external_script_source_size",
        statistics.external_script_source_size(),
    )?;
    result.set_item(
        "cpu_profiler_metadata_size",
        statistics.cpu_profiler_metadata_size(),
    )?;

    Ok(result.into_any().unbind())
}

pub(crate) fn memory_pressure(isolate: &SharedIsolate, level: &str) -> PyResult<()> {
    let level = match level {
        "none" => v8::MemoryPressureLevel::None,
        "moderate" => v8::MemoryPressureLevel::Moderate,
        "critical" => v8::MemoryPressureLevel::Critical,
        _ => {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "memory pressure level must be 'none', 'moderate', or 'critical'.",
            ));
        }
    };

    isolate.borrow_mut().memory_pressure_notification(level);
    Ok(())
}

pub(crate) fn low_memory_notification(isolate: &SharedIsolate) {
    isolate.borrow_mut().low_memory_notification();
}

pub(crate) fn request_garbage_collection_for_testing(
    isolate: &SharedIsolate,
    collection_type: &str,
) -> PyResult<()> {
    let collection_type = match collection_type {
        "full" => v8::GarbageCollectionType::Full,
        "minor" => v8::GarbageCollectionType::Minor,
        _ => {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "garbage collection type must be 'full' or 'minor'.",
            ));
        }
    };

    isolate
        .borrow_mut()
        .request_garbage_collection_for_testing(collection_type);
    Ok(())
}
