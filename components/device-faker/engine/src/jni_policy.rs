//! Keep optional field failures from escaping into application startup.
use jni::{
    Env,
    errors::{ErrorPolicy, Result},
};
pub struct ClearAndDefault;
impl<T: Default, E: std::fmt::Display> ErrorPolicy<T, E> for ClearAndDefault {
    type Captures<'local: 'native, 'native> = ();
    fn on_error<'local: 'native, 'native>(
        env: &mut Env<'local>,
        _: &mut (),
        error: E,
    ) -> Result<T> {
        env.exception_clear();
        log::warn!("JNI profile skipped: {error}");
        Ok(T::default())
    }
    fn on_panic<'local: 'native, 'native>(
        env: &mut Env<'local>,
        _: &mut (),
        payload: Box<dyn std::any::Any + Send>,
    ) -> Result<T> {
        env.exception_clear();
        std::mem::forget(payload);
        log::warn!("JNI profile callback failed");
        Ok(T::default())
    }
}
