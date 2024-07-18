//! Macros for the hypervisor.

/// A macro for formatted printing without a newline.
///
/// This macro is a reimplementation of the standard `print!` macro, redirecting
/// the output to a custom print handler defined in the `_print` function from
/// the `utils` module.
///
/// # Examples
///
/// ```
/// # use my_crate::print;
/// print!("Hello, {}!", "world");
/// ```
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::utils::_print(format_args!($($arg)*)));
}

/// A macro for formatted printing with a newline.
///
/// This macro is similar to the standard `println!` macro, redirecting the output
/// to a custom print handler defined in the `_print` function from the `utils` module.
///
/// # Examples
///
/// ```
/// # use my_crate::println;
/// println!("Hello, {}!", "world");
/// ```
#[macro_export]
macro_rules! println {
    () => (print!("\n"));
    ($($arg:tt)*) => (print!("{}\n", format_args!($($arg)*)));
}

/// A macro for declaring an enum with associated handlers.
///
/// This macro simplifies the declaration of an enum with associated handlers.
/// It generates both the enum variants and a static array containing the
/// corresponding handlers.
///
/// # Examples
///
/// ```
/// # use my_crate::declare_enum_with_handler;
/// declare_enum_with_handler! {
///     /// An example enum with handlers.
///     pub enum MyEnum [pub HANDLERS => HandlerType] {
///         Variant1 => handler_function1,
///         Variant2 => handler_function2,
///     }
/// }
/// ```
#[macro_export]
macro_rules! declare_enum_with_handler {
    (
        $(#[$attr:meta])*
        $enum_vis:vis enum $enum_name:ident [$array_vis:vis $array:ident => $handler_type:ty] {
            $($vis:vis $variant:ident => $handler:expr, )*
        }
    ) => {
        $(#[$attr])*
        $enum_vis enum $enum_name {
            $($vis $variant, )*
        }
        $array_vis static $array: &[$handler_type] = &[
            $($handler, )*
        ];
    }
}
