#[macro_export]
macro_rules! try_opt {
    ($expr:expr) => (match $expr { Some(e) => e, None => return None })
}

#[macro_export]
macro_rules! unwrap {
   ($expr:expr) => (($expr).unwrap_or_else(|| ::ffi::really_bad(concat!("Expression did not unwrap: ", stringify!($expr)).into())))
}

#[macro_export]
macro_rules! unwrap1 {
   ($expr:expr) => (($expr).unwrap_or_else(|_| ::ffi::really_bad(concat!("Expression did not unwrap: ", stringify!($expr)).into())))
}
