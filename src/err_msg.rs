use std::fmt::Display;
pub trait WithErrMsg<T> {
    fn with_err_msg<U: Display>(self, msg: &U) -> Result<T, String>;
}
impl<T> WithErrMsg<T> for Option<T> {
    fn with_err_msg<U: Display>(self, msg: &U) -> Result<T, String> {
        match self {
            Some(val) => Ok(val),
            None => Err(msg.to_string()),
        }
    }
}
impl<T, E: Display> WithErrMsg<T> for Result<T, E> {
    fn with_err_msg<U: Display>(self, msg: &U) -> Result<T, String> {
        match self {
            Ok(val) => Ok(val),
            Err(_) => Err(msg.to_string()),
        }
    }
}
