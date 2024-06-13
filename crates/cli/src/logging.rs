#[macro_export]
macro_rules! log_warning {
    ($msg:literal) => {
        eprint!("warning: ");
        eprintln!($msg);
    };
}
