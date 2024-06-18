#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeId(usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OutputId(usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InputId(usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PinId {
    Output(OutputId),
    Input(InputId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeOutput {
    pub node_id: NodeId,
    pub output_id: OutputId,
    pub type_id: TypeId,
}

impl NodeOutput {
    pub fn new(node_id: NodeId, output_id: OutputId, type_id: TypeId) -> Self {
        Self {
            node_id,
            output_id,
            type_id,
        }
    }
}

#[derive(Debug)]
pub struct NodeInput<T> {
    value: T,
    connection: Option<NodeOutput>,
}

impl<T> NodeInput<T> {
    pub fn new(value: T) -> Self {
        Self {
            value,
            connection: None,
        }
    }

    pub fn connection(&self) -> Option<NodeOutput> {
        self.connection
    }

    pub fn value(&self) -> &T {
        &self.value
    }

    pub fn value_mut(&mut self) -> &mut T {
        &mut self.value
    }

    pub fn connect(&mut self, connection: NodeOutput) {
        self.connection = Some(connection);
    }

    pub fn disconnect(&mut self) {
        self.connection = None;
    }
}

impl<T: Default> Default for NodeInput<T> {
    fn default() -> Self {
        Self {
            value: T::default(),
            connection: None,
        }
    }
}

macro_rules! impl_enum_from_into_id_types {
    ($t:tt, [ $($id_type:ty),+ ], { $($index:expr => $variant:ident),+$(,)? }) => {
        impl_enum_from_into_id_types!(@call_tuple $t, { $($id_type ),+ } ( $($index, $variant),+ ));
    };
    (@call_tuple $t:tt, { $($id_type:ty ),+ } $tuple:tt ) => {
        $(impl_enum_from_into_id_types!(@call $t, $id_type, $tuple);)+
    };
    (@call $t:tt, $id_type:ty, ( $($index:expr, $variant:ident),+ )) => {
        impl From<$id_type> for $t {
            fn from(id_val: $id_type) -> Self {
                match id_val.into() {
                    $($index => $t::$variant),+,
                    _ => unreachable!(),
                }
            }
        }

        impl Into<$id_type> for $t {
            fn into(self) -> $id_type {
                match self {
                    $($t::$variant => $index),+,
                }
                .into()
            }
        }
    };
}

pub(crate) use impl_enum_from_into_id_types;

macro_rules! impl_from_into_usize {
    ($t:ty) => {
        impl From<usize> for $t {
            fn from(id: usize) -> Self {
                Self(id)
            }
        }

        impl Into<usize> for $t {
            fn into(self) -> usize {
                self.0
            }
        }
    };
    ($t:ty, $($rest:ty),+) => {
        impl_from_into_usize!($t);
        impl_from_into_usize!($($rest),+);
    };
}

impl_from_into_usize!(NodeId, TypeId, OutputId, InputId);

impl From<OutputId> for PinId {
    fn from(id: OutputId) -> Self {
        Self::Output(id)
    }
}

impl From<InputId> for PinId {
    fn from(id: InputId) -> Self {
        Self::Input(id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InputIdNone;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InputIdSingle;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OutputIdNone;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OutputIdSingle;

macro_rules! impl_id_variants {
    ($t:ty, $id:ty) => {
        impl From<$id> for $t {
            fn from(_: $id) -> Self {
                Self
            }
        }

        impl Into<$id> for $t {
            fn into(self) -> $id {
                0.into()
            }
        }
    };
    ({ $(($t:ty,$id:ty)),+ }) => {
        $(impl_id_variants!($t, $id);)+
    };
}

impl_id_variants!({
    (InputIdNone, InputId),
    (InputIdSingle, InputId),
    (OutputIdNone, OutputId),
    (OutputIdSingle, OutputId)
});
