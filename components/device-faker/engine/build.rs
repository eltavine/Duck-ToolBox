//! 构建期用 rsbinder-aidl 把前台观察相关的 AIDL 编译成 Rust 绑定。
//!
//! - `IUidObserver`：系统侧回调我们的接口（onUidStateChanged 等），事务码与
//!   真实 AOSP `android.app.IUidObserver` 一致（1..=6）。
//! - `IActivityManager`：最小代理，`registerUidObserver` 必须保持在接口第 2 位
//!   （code=2），与真实 IActivityManager 的 native 侧稳定区段一致。

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=aidl/android/app/IUidObserver.aidl");
    println!("cargo:rerun-if-changed=aidl/android/app/IActivityManager.aidl");

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR must be set by cargo");

    // 两个接口同属 android.app 包，一次生成避免重复 pub mod android::app。
    // 必须 async support（rsbinder 0.5.3 的 declare_binder_interface! 在 async
    // feature 下才生成 BnX::new_binder 同步服务；见 Cargo.toml 注释）。
    rsbinder_aidl::Builder::new()
        .include_dir(PathBuf::from("aidl"))
        .set_async_support(true)
        .source(PathBuf::from("aidl/android/app/IUidObserver.aidl"))
        .source(PathBuf::from("aidl/android/app/IActivityManager.aidl"))
        .output(PathBuf::from(&out_dir).join("fg_binder.rs"))
        .generate()
        .expect("failed to generate fg binder bindings");
}
