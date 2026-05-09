use pyo3::exceptions::PyRuntimeWarning;
use pyo3::prelude::{Bound, PyAny, PyResult, Python};
use pyo3::types::PyAnyMethods;

use super::convert::python_to_v8;
use super::handle::V8Value;
use super::kind::{ValueKind, classify_value};
use super::value::Value;
use crate::error::js_exception;

#[derive(Clone, Copy)]
pub(super) enum BinaryOperator {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Pow,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl BinaryOperator {
    fn symbol(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Sub => "-",
            Self::Mul => "*",
            Self::Div => "/",
            Self::Rem => "%",
            Self::Pow => "**",
            Self::BitAnd => "&",
            Self::BitOr => "|",
            Self::BitXor => "^",
            Self::Shl => "<<",
            Self::Shr => ">>",
            Self::Eq => "==",
            Self::Ne => "!=",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Gt => ">",
            Self::Ge => ">=",
        }
    }

    fn source(self) -> &'static str {
        match self {
            Self::Add => "(function(a, b) { return a + b; })",
            Self::Sub => "(function(a, b) { return a - b; })",
            Self::Mul => "(function(a, b) { return a * b; })",
            Self::Div => "(function(a, b) { return a / b; })",
            Self::Rem => "(function(a, b) { return a % b; })",
            Self::Pow => "(function(a, b) { return a ** b; })",
            Self::BitAnd => "(function(a, b) { return a & b; })",
            Self::BitOr => "(function(a, b) { return a | b; })",
            Self::BitXor => "(function(a, b) { return a ^ b; })",
            Self::Shl => "(function(a, b) { return a << b; })",
            Self::Shr => "(function(a, b) { return a >> b; })",
            Self::Eq => "(function(a, b) { return a == b; })",
            Self::Ne => "(function(a, b) { return a != b; })",
            Self::Lt => "(function(a, b) { return a < b; })",
            Self::Le => "(function(a, b) { return a <= b; })",
            Self::Gt => "(function(a, b) { return a > b; })",
            Self::Ge => "(function(a, b) { return a >= b; })",
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum UnaryOperator {
    Pos,
    Neg,
    Invert,
}

impl UnaryOperator {
    fn source(self) -> &'static str {
        match self {
            Self::Pos => "(function(a) { return +a; })",
            Self::Neg => "(function(a) { return -a; })",
            Self::Invert => "(function(a) { return ~a; })",
        }
    }
}

pub(super) fn apply_binary_value_operator(
    handle: &V8Value,
    py: Python<'_>,
    other: &Bound<'_, PyAny>,
    operator: BinaryOperator,
    reverse: bool,
) -> PyResult<Value> {
    let mut isolate = handle.isolate.borrow_mut();
    let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate));
    let scope = &mut scope.init();
    let context = v8::Local::new(scope, &handle.context);
    let scope = &mut v8::ContextScope::new(scope, context);
    v8::tc_scope!(let scope, &mut **scope);

    let lhs = v8::Local::new(scope, &handle.value);
    let rhs = python_to_v8(py, scope, other, handle.isolate_id, 0)?;
    let lhs_kind = classify_value(lhs);
    let rhs_kind = classify_value(rhs);
    let (left_kind, right_kind) = if reverse {
        (rhs_kind, lhs_kind)
    } else {
        (lhs_kind, rhs_kind)
    };
    warn_for_mixed_operand_types(py, operator, left_kind, right_kind)?;

    let source = v8::String::new(scope, operator.source()).ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to create operator function source.")
    })?;
    let script = v8::Script::compile(scope, source, None)
        .ok_or_else(|| js_exception(scope, "Failed to compile operator function."))?;
    let function = script
        .run(scope)
        .ok_or_else(|| js_exception(scope, "Failed to create operator function."))?;
    let function = v8::Local::<v8::Function>::try_from(function).map_err(|_| {
        pyo3::exceptions::PyRuntimeError::new_err("Operator source did not evaluate to a function.")
    })?;
    let (left, right) = if reverse { (rhs, lhs) } else { (lhs, rhs) };
    let args = [left, right];
    let recv = v8::undefined(scope).into();
    let result = function
        .call(scope, recv, &args)
        .ok_or_else(|| js_exception(scope, "Operator execution failed."))?;

    Ok(Value::from_local(
        scope,
        result,
        handle.context.clone(),
        handle.isolate.clone(),
        handle.isolate_id,
    ))
}

pub(super) fn apply_binary_bool_operator(
    handle: &V8Value,
    py: Python<'_>,
    other: &Bound<'_, PyAny>,
    operator: BinaryOperator,
) -> PyResult<bool> {
    let result = apply_binary_value_operator(handle, py, other, operator, false)?;

    result.handle.with_local_value(|scope, value| {
        if !value.is_boolean() {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "Operator did not return a boolean.",
            ));
        }

        Ok(value.boolean_value(scope))
    })
}

pub(super) fn apply_unary_value_operator(
    handle: &V8Value,
    operator: UnaryOperator,
) -> PyResult<Value> {
    let mut isolate = handle.isolate.borrow_mut();
    let scope = std::pin::pin!(v8::HandleScope::new(&mut **isolate));
    let scope = &mut scope.init();
    let context = v8::Local::new(scope, &handle.context);
    let scope = &mut v8::ContextScope::new(scope, context);
    v8::tc_scope!(let scope, &mut **scope);

    let value = v8::Local::new(scope, &handle.value);
    let source = v8::String::new(scope, operator.source()).ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to create operator function source.")
    })?;
    let script = v8::Script::compile(scope, source, None)
        .ok_or_else(|| js_exception(scope, "Failed to compile operator function."))?;
    let function = script
        .run(scope)
        .ok_or_else(|| js_exception(scope, "Failed to create operator function."))?;
    let function = v8::Local::<v8::Function>::try_from(function).map_err(|_| {
        pyo3::exceptions::PyRuntimeError::new_err("Operator source did not evaluate to a function.")
    })?;
    let args = [value];
    let recv = v8::undefined(scope).into();
    let result = function
        .call(scope, recv, &args)
        .ok_or_else(|| js_exception(scope, "Operator execution failed."))?;

    Ok(Value::from_local(
        scope,
        result,
        handle.context.clone(),
        handle.isolate.clone(),
        handle.isolate_id,
    ))
}

fn warn_for_mixed_operand_types(
    py: Python<'_>,
    operator: BinaryOperator,
    left_kind: ValueKind,
    right_kind: ValueKind,
) -> PyResult<()> {
    if left_kind.has_same_operator_type(right_kind) {
        return Ok(());
    }

    let message = format!(
        "JavaScript operator '{}' was used with mixed operand types '{}' and '{}'; JavaScript coercion semantics were applied.",
        operator.symbol(),
        left_kind.operator_type_name(),
        right_kind.operator_type_name()
    );
    let warnings = py.import("warnings")?;
    let category = py.get_type::<PyRuntimeWarning>();
    warnings.call_method1("warn", (message, category, 2))?;

    Ok(())
}
