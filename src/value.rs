/// Leaf value storable in Node

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum Value {
    /// Represents a JSON null value
    Null,

    /// Represents a JSON boolean
    Bool(bool),

    /// Represents a JSON signed integer
    I64(i64),

    /// Represents a JSON unsigned integer
    U64(u64),

    /// Represents a JSON floating point number
    F64(f64),

    /// Represents a JSON string
    String(Box<str>)
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::String(s.into_boxed_str())
    }
}
