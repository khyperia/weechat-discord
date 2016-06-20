#[macro_export]
macro_rules! try_opt {
    ($expr:expr) => (match $expr { Some(e) => e, None => return None })
}

// #[macro_export]
// macro_rules! try_opt_ref {
//    ($expr:expr) => (match *$expr { Some(ref e) => e, None => return None })
// }
