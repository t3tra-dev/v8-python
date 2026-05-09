#[derive(Clone, Copy)]
pub(crate) enum ValueKind {
    Undefined,
    Null,
    Boolean,
    Int32,
    Uint32,
    Number,
    BigInt,
    String,
    Symbol,
    Array,
    Function,
    Promise,
    ArrayBuffer,
    ArrayBufferView,
    TypedArray,
    DataView,
    Map,
    Set,
    Date,
    RegExp,
    Proxy,
    External,
    WasmModule,
    Object,
}

impl ValueKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Undefined => "undefined",
            Self::Null => "null",
            Self::Boolean => "boolean",
            Self::Int32 => "int32",
            Self::Uint32 => "uint32",
            Self::Number => "number",
            Self::BigInt => "bigint",
            Self::String => "string",
            Self::Symbol => "symbol",
            Self::Array => "array",
            Self::Function => "function",
            Self::Promise => "promise",
            Self::ArrayBuffer => "array_buffer",
            Self::ArrayBufferView => "array_buffer_view",
            Self::TypedArray => "typed_array",
            Self::DataView => "data_view",
            Self::Map => "map",
            Self::Set => "set",
            Self::Date => "date",
            Self::RegExp => "regexp",
            Self::Proxy => "proxy",
            Self::External => "external",
            Self::WasmModule => "wasm_module",
            Self::Object => "object",
        }
    }

    pub(crate) fn is_number(self) -> bool {
        matches!(self, Self::Int32 | Self::Uint32 | Self::Number)
    }

    pub(crate) fn is_object(self) -> bool {
        matches!(
            self,
            Self::Array
                | Self::Function
                | Self::Promise
                | Self::ArrayBuffer
                | Self::ArrayBufferView
                | Self::TypedArray
                | Self::DataView
                | Self::Map
                | Self::Set
                | Self::Date
                | Self::RegExp
                | Self::Proxy
                | Self::WasmModule
                | Self::Object
        )
    }

    pub(crate) fn operator_type_name(self) -> &'static str {
        match self {
            Self::Undefined => "undefined",
            Self::Null => "null",
            Self::Boolean => "boolean",
            Self::Int32 | Self::Uint32 | Self::Number => "number",
            Self::BigInt => "bigint",
            Self::String => "string",
            Self::Symbol => "symbol",
            Self::Array
            | Self::Function
            | Self::Promise
            | Self::ArrayBuffer
            | Self::ArrayBufferView
            | Self::TypedArray
            | Self::DataView
            | Self::Map
            | Self::Set
            | Self::Date
            | Self::RegExp
            | Self::Proxy
            | Self::External
            | Self::WasmModule
            | Self::Object => "object",
        }
    }

    pub(crate) fn has_same_operator_type(self, other: Self) -> bool {
        self.operator_type_name() == other.operator_type_name()
    }
}

pub(crate) fn classify_value(value: v8::Local<'_, v8::Value>) -> ValueKind {
    if value.is_undefined() {
        return ValueKind::Undefined;
    }

    if value.is_null() {
        return ValueKind::Null;
    }

    if value.is_boolean() {
        return ValueKind::Boolean;
    }

    if value.is_int32() {
        return ValueKind::Int32;
    }

    if value.is_uint32() {
        return ValueKind::Uint32;
    }

    if value.is_number() {
        return ValueKind::Number;
    }

    if value.is_big_int() {
        return ValueKind::BigInt;
    }

    if value.is_string() {
        return ValueKind::String;
    }

    if value.is_symbol() {
        return ValueKind::Symbol;
    }

    if value.is_array() {
        return ValueKind::Array;
    }

    if value.is_function() {
        return ValueKind::Function;
    }

    if value.is_promise() {
        return ValueKind::Promise;
    }

    if value.is_map() {
        return ValueKind::Map;
    }

    if value.is_set() {
        return ValueKind::Set;
    }

    if value.is_date() {
        return ValueKind::Date;
    }

    if value.is_reg_exp() {
        return ValueKind::RegExp;
    }

    if value.is_proxy() {
        return ValueKind::Proxy;
    }

    if value.is_external() {
        return ValueKind::External;
    }

    if value.is_wasm_module_object() {
        return ValueKind::WasmModule;
    }

    if value.is_array_buffer() {
        return ValueKind::ArrayBuffer;
    }

    if value.is_data_view() {
        return ValueKind::DataView;
    }

    if value.is_typed_array() {
        return ValueKind::TypedArray;
    }

    if value.is_array_buffer_view() {
        return ValueKind::ArrayBufferView;
    }

    ValueKind::Object
}

pub(crate) fn promise_state_name(state: &v8::PromiseState) -> &'static str {
    match state {
        v8::PromiseState::Pending => "pending",
        v8::PromiseState::Fulfilled => "fulfilled",
        v8::PromiseState::Rejected => "rejected",
    }
}
