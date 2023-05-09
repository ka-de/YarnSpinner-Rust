use crate::prelude::*;
use std::any::TypeId;
use std::fmt::{Debug, Formatter};
use std::marker::PhantomData;
use yarn_slinger_macros::all_tuples;

/// A function that can be registered into and called from Yarn.
/// It must have the following properties:
/// - It is allowed to have zero or more parameters
/// - Each parameter must be one of the following types or a reference to them:
///   - [`bool`]
///   - A numeric type or its reference, i.e. one of [`f32`], [`f64`], [`i8`], [`i16`], [`i32`], [`i64`], [`i128`], [`u8`], [`u16`], [`u32`], [`u64`], [`u128`], [`usize`], [`isize`],
///   - [`String`] (for a reference, [`&str`] may be used instead of [`&String`])
///   - [`YarnValue`], which means that a parameter may be any of the above types
/// - It must return a value.
/// - Its return type must be one of the types listed above, but neither a reference nor a [`YarnValue`].
/// ## Examples
/// ```rust
/// fn give_summary(name: &str, age: usize, is_cool: bool) -> String {
///    format!("{name} is {age} years old and is {} cool", if is_cool { "very" } else { "not" })
/// }
/// ```
/// Which may be called from Yarn as follows:
/// ```yarn
/// <<set $name to "Bob">>
/// <<set $age to 42>>
/// <<set $is_cool to true>>
/// Narrator: {give_summary($name, $age, $is_cool)}
/// ```
pub trait YarnFn<Marker>: Clone + Send + Sync {
    type Out: IntoYarnValueFromNonYarnValue + 'static;
    fn call(&self, input: Vec<YarnValue>) -> Self::Out;
    fn parameter_types(&self) -> Vec<TypeId>;
    fn return_type(&self) -> TypeId {
        TypeId::of::<Self::Out>()
    }
}

/// A [`YarnFn`] with the `Marker` type parameter erased.
/// See its documentation for more information about what kind of functions are allowed.
pub trait UntypedYarnFn: Debug + Send + Sync {
    fn call(&self, input: Vec<YarnValue>) -> YarnValue;
    fn clone_box(&self) -> Box<dyn UntypedYarnFn + Send + Sync>;
    fn parameter_types(&self) -> Vec<TypeId>;
    fn return_type(&self) -> TypeId;
}

impl Clone for Box<dyn UntypedYarnFn + Send + Sync> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

impl<Marker, F> UntypedYarnFn for YarnFnWrapper<Marker, F>
where
    Marker: 'static + Clone,
    F: YarnFn<Marker> + 'static + Clone + Send + Sync,
    F::Out: IntoYarnValueFromNonYarnValue + 'static + Clone,
{
    fn call(&self, input: Vec<YarnValue>) -> YarnValue {
        let output = self.function.call(input);
        output.into_untyped_value()
    }

    fn clone_box(&self) -> Box<dyn UntypedYarnFn + Send + Sync> {
        Box::new(self.clone())
    }

    fn parameter_types(&self) -> Vec<TypeId> {
        self.function.parameter_types()
    }

    fn return_type(&self) -> TypeId {
        self.function.return_type()
    }
}

#[derive(Clone)]
pub(crate) struct YarnFnWrapper<Marker, F>
where
    F: YarnFn<Marker>,
{
    function: F,

    // NOTE: PhantomData<fn()-> T> gives this safe Send/Sync impls
    _marker: PhantomData<fn() -> Marker>,
}

impl<Marker, F> From<F> for YarnFnWrapper<Marker, F>
where
    F: YarnFn<Marker>,
{
    fn from(function: F) -> Self {
        Self {
            function,
            _marker: PhantomData,
        }
    }
}

impl<Marker, F> Debug for YarnFnWrapper<Marker, F>
where
    F: YarnFn<Marker>,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let signature = std::any::type_name::<Marker>();
        let function_path = std::any::type_name::<F>();
        let debug_message = format!("{signature} {{{function_path}}}");
        f.debug_struct(&debug_message).finish()
    }
}

impl PartialEq for Box<dyn UntypedYarnFn + Send + Sync> {
    fn eq(&self, other: &Self) -> bool {
        // Not guaranteed to be unique, but it's good enough for our purposes.
        let debug = format!("{:?}", self);
        let other_debug = format!("{:?}", other);
        debug == other_debug
    }
}

impl Eq for Box<dyn UntypedYarnFn + Send + Sync> {}

/// Adapted from <https://github.com/bevyengine/bevy/blob/fe852fd0adbce6856f5886d66d20d62cfc936287/crates/bevy_ecs/src/system/system_param.rs#L1370>
macro_rules! impl_yarn_fn_tuple {
    ($($param: ident),*) => {
        #[allow(non_snake_case)]
        impl<F, O, $($param,)*> YarnFn<fn($($param,)*) -> O> for F
            where
            for <'a>F:
                Send + Sync + Clone +
                Fn($($param,)*) -> O +
                Fn($(<$param as YarnFnParam>::Item<'a>,)*) -> O,
            O: IntoYarnValueFromNonYarnValue + 'static,
            $($param: YarnFnParam + 'static,)*
            {
                type Out = O;
                #[allow(non_snake_case)]
                fn call(&self, input: Vec<YarnValue>) -> Self::Out {
                    // Hack: mapping to Option to be able to tuple deconstruct by moving
                    let mut input_options = input.into_iter().map(Some).collect::<Vec<_>>();
                    // Tuple deconstruct to &mut Option<YarnValue>
                    let [$($param,)*] = &mut input_options[..] else {
                        panic!("Wrong number of arguments")
                    };
                    // `take` the YarnValue out of the Option, leaving None in its place
                    let ($($param,)*) = (
                        $(std::mem::take($param).unwrap(),)*
                    );
                    // Now $param holds an owned YarnValue!

                    let ($(mut $param,)*) = (
                        $(YarnValueWrapper::from($param),)*
                    );

                    // the first $param is the type implementing YarnFnParam, the second is a variable name
                    let input = (
                        $($param::retrieve(&mut $param),)*
                    );
                    let ($($param,)*) = input;
                    self($($param,)*)
                }

                fn parameter_types(&self) -> Vec<TypeId> {
                    vec![$(TypeId::of::<$param>()),*]
                }
            }
    };
}

all_tuples!(impl_yarn_fn_tuple, 0, 16, P);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_no_params() {
        fn f() -> bool {
            true
        }
        accept_yarn_fn(f);
    }

    #[test]
    fn accepts_string() {
        fn f(_: String) -> bool {
            true
        }
        accept_yarn_fn(f);
    }

    #[test]
    fn accepts_string_ref() {
        fn f(_: &String) -> bool {
            true
        }
        accept_yarn_fn(f);
    }

    #[test]
    fn accepts_string_slice() {
        fn f(_: &str) -> bool {
            true
        }
        accept_yarn_fn(f);
    }

    #[test]
    fn accepts_usize() {
        fn f(_: usize) -> bool {
            true
        }
        accept_yarn_fn(f);
    }

    #[test]
    fn accepts_usize_ref() {
        fn f(_: &usize) -> bool {
            true
        }
        accept_yarn_fn(f);
    }

    #[test]
    fn accepts_yarn_value() {
        fn f(_: YarnValue) -> bool {
            true
        }
        accept_yarn_fn(f);
    }

    #[test]
    fn accepts_yarn_value_ref() {
        fn f(_: &YarnValue) -> bool {
            true
        }
        accept_yarn_fn(f);
    }

    #[test]
    fn accepts_multiple_strings() {
        fn f(s: String, _: String, _: &str, _: String, _: &str) -> String {
            s
        }
        accept_yarn_fn(f);
    }

    #[test]
    fn accepts_lots_of_different_types() {
        #[allow(clippy::too_many_arguments)]
        fn f(
            _: String,
            _: usize,
            _: &str,
            _: &YarnValue,
            _: &bool,
            _: isize,
            _: String,
            _: &u32,
        ) -> bool {
            true
        }
        accept_yarn_fn(f);
    }

    fn accept_yarn_fn<Marker>(_: impl YarnFn<Marker>) {}
}
