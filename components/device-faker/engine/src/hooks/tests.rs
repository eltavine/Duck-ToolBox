use super::*;
use std::{
    ffi::{CString, c_void},
    path::PathBuf,
    process::Command,
};

// One JVM per test process; the fixture deliberately models Android before API 31.
#[test]
fn missing_build_fields_are_skipped_without_losing_later_fields_or_exceptions() {
    let java_home = std::env::var_os("JAVA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            if cfg!(target_os = "macos") {
                PathBuf::from(
                    String::from_utf8(
                        Command::new("/usr/libexec/java_home")
                            .output()
                            .unwrap()
                            .stdout,
                    )
                    .unwrap()
                    .trim(),
                )
            } else {
                std::fs::canonicalize(
                    Command::new("sh")
                        .args(["-c", "command -v javac"])
                        .output()
                        .map(|o| String::from_utf8(o.stdout).unwrap().trim().to_owned())
                        .unwrap(),
                )
                .unwrap()
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .to_owned()
            }
        });
    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("android/os/Build.java");
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::write(
        &source,
        r#"package android.os;
public class Build {
    public static String MODEL = "stock";
    public static String FINGERPRINT = "stock";
    public static class VERSION {
        public static int SDK_INT = 30;
        public static String SDK = "30";
        public static String INCREMENTAL = "stock";
    }
}"#,
    )
    .unwrap();
    assert!(
        Command::new(java_home.join("bin/javac"))
            .args(["-d"])
            .arg(temporary.path())
            .arg(&source)
            .status()
            .unwrap()
            .success()
    );
    let library = java_home.join(if cfg!(target_os = "macos") {
        "lib/server/libjvm.dylib"
    } else {
        "lib/server/libjvm.so"
    });
    let path = CString::new(library.to_str().unwrap()).unwrap();
    // The library and VM intentionally live until process exit; JNI keeps global state.
    let handle = unsafe { libc::dlopen(path.as_ptr(), libc::RTLD_NOW | libc::RTLD_GLOBAL) };
    assert!(!handle.is_null(), "JVM unavailable: {}", library.display());
    type Create =
        unsafe extern "system" fn(*mut *mut jni::sys::JavaVM, *mut *mut c_void, *mut c_void) -> i32;
    let symbol = unsafe { libc::dlsym(handle, c"JNI_CreateJavaVM".as_ptr()) };
    assert!(!symbol.is_null());
    let create: Create = unsafe { std::mem::transmute(symbol) };
    let classpath =
        CString::new(format!("-Djava.class.path={}", temporary.path().display())).unwrap();
    let mut option = jni::sys::JavaVMOption {
        optionString: classpath.as_ptr().cast_mut(),
        extraInfo: std::ptr::null_mut(),
    };
    let mut init = jni::sys::JavaVMInitArgs {
        version: jni::sys::JNI_VERSION_1_8,
        nOptions: 1,
        options: &mut option,
        ignoreUnrecognized: false,
    };
    let mut vm = std::ptr::null_mut();
    let mut raw_env = std::ptr::null_mut();
    assert_eq!(
        unsafe {
            create(
                &mut vm,
                &mut raw_env,
                (&mut init as *mut jni::sys::JavaVMInitArgs).cast(),
            )
        },
        0
    );
    let mut env = unsafe { EnvUnowned::from_raw(raw_env.cast()) };
    let config = crate::config::Config::from_toml("[[apps]]\npackage='com.example.app'\nmodel='Phone'\nsoc_model='New SoC'\nfingerprint='New fingerprint'\nincremental='123'\nsdk_int=35\ncustom_props={'ro.product.model'='Custom phone', 'ro.build.fingerprint'='Custom fingerprint', 'ro.build.version.sdk'='36'}\n").unwrap();
    let merged = config.get_merged_config("com.example.app").unwrap();
    let props = crate::config::Config::build_merged_property_map_with_explicit(&merged).0;
    hook_build_fields(&mut env, &props).unwrap();
    let passed = env
        .with_env(|env| -> jni::errors::Result<bool> {
            assert!(!env.exception_check());
            let fingerprint = env
                .get_static_field(
                    jni_str!("android/os/Build"),
                    jni_str!("FINGERPRINT"),
                    jni_sig!("Ljava/lang/String;"),
                )?
                .l()?;
            let text = env
                .call_method(
                    &fingerprint,
                    jni_str!("toString"),
                    jni_sig!("()Ljava/lang/String;"),
                    &[],
                )?
                .l()?;
            let string = env.cast_local::<jni::objects::JString>(text)?;
            assert_eq!(string.to_string(), "Custom fingerprint");
            let sdk = env
                .get_static_field(
                    jni_str!("android/os/Build$VERSION"),
                    jni_str!("SDK"),
                    jni_sig!("Ljava/lang/String;"),
                )?
                .l()?;
            assert_eq!(
                env.cast_local::<jni::objects::JString>(sdk)?.to_string(),
                "36"
            );
            assert_eq!(
                env.get_static_field(
                    jni_str!("android/os/Build$VERSION"),
                    jni_str!("SDK_INT"),
                    jni_sig!("I")
                )?
                .i()?,
                36
            );
            let _ = env.throw_new(
                jni_str!("java/lang/IllegalStateException"),
                jni_str!("original exception"),
            );
            Ok(true)
        })
        .resolve::<crate::jni_policy::ClearAndDefault>();
    assert!(passed);
    assert!(hook_build_fields(&mut env, &props).is_err());
    let retained = env
        .with_env(|env| -> jni::errors::Result<bool> {
            let exception = env
                .exception_occurred()
                .expect("preexisting exception must remain pending");
            env.exception_clear();
            env.is_instance_of(exception, jni_str!("java/lang/IllegalStateException"))
        })
        .resolve::<crate::jni_policy::ClearAndDefault>();
    assert!(retained);
    let mut invalid = props.clone();
    invalid.insert("ro.product.model".into(), "Must not be applied".into());
    invalid.insert("ro.build.version.sdk".into(), "invalid".into());
    assert!(hook_build_fields(&mut env, &invalid).is_err());
    let mut empty = props.clone();
    empty.remove("ro.product.model");
    empty.remove("ro.build.version.sdk");
    empty.insert("ro.build.fingerprint".into(), String::new());
    hook_build_fields(&mut env, &empty).unwrap();
    let final_fields = env
        .with_env(|env| -> jni::errors::Result<bool> {
            assert!(!env.exception_check());
            for (field, expected) in [
                (jni_str!("MODEL"), "Custom phone"),
                (jni_str!("FINGERPRINT"), ""),
            ] {
                let value = env
                    .get_static_field(
                        jni_str!("android/os/Build"),
                        field,
                        jni_sig!("Ljava/lang/String;"),
                    )?
                    .l()?;
                assert_eq!(
                    env.cast_local::<jni::objects::JString>(value)?.to_string(),
                    expected
                );
            }
            assert_eq!(
                env.get_static_field(
                    jni_str!("android/os/Build$VERSION"),
                    jni_str!("SDK_INT"),
                    jni_sig!("I")
                )?
                .i()?,
                36
            );
            Ok(true)
        })
        .resolve::<crate::jni_policy::ClearAndDefault>();
    assert!(final_fields);
}
