// SPDX-License-Identifier: GPL-3.0-only
#include "language.hpp"
#include "lsplant.hpp"
#include "shadowhook.h"
#include "xdl.h"
#include <android/log.h>
#include <map>
#include <mutex>

extern "C" void *duck_xdl_dsym_prefix(void *, const char *);

namespace duck::language {
void install_properties(const std::string &locale);
// C inline hooks cannot carry user data. This adapter owns only LSPlant's native
// trampolines in this specialized process; feature configuration stays in Java.
static std::map<void *, void *> trampolines;
static std::mutex trampoline_lock;
static jobject hook(JNIEnv *env, jclass, jobject method, jobject owner, jobject callback) {
    return lsplant::Hook(env, method, owner, callback);
}
static jboolean deoptimize(JNIEnv *env, jclass, jobject method) {
    return lsplant::Deoptimize(env, method);
}
static jclass load(JNIEnv *env, std::vector<char> &dex) {
    auto loaders = env->FindClass("java/lang/ClassLoader");
    if (!loaders) return nullptr;
    auto system = env->GetStaticMethodID(loaders, "getSystemClassLoader", "()Ljava/lang/ClassLoader;");
    if (!system) return nullptr;
    auto parent = env->CallStaticObjectMethod(loaders, system);
    if (!parent || env->ExceptionCheck()) return nullptr;
    auto clazz = env->FindClass("dalvik/system/InMemoryDexClassLoader");
    if (!clazz) return nullptr;
    auto constructor = env->GetMethodID(clazz, "<init>", "(Ljava/nio/ByteBuffer;Ljava/lang/ClassLoader;)V");
    if (!constructor || env->ExceptionCheck()) return nullptr;
    auto buffer = env->NewDirectByteBuffer(dex.data(), static_cast<jlong>(dex.size()));
    if (!buffer || env->ExceptionCheck()) return nullptr;
    auto loader = env->NewObject(clazz, constructor, buffer, parent);
    if (!loader || env->ExceptionCheck()) return nullptr;
    auto method = env->GetMethodID(loaders, "loadClass", "(Ljava/lang/String;)Ljava/lang/Class;");
    if (!method) return nullptr;
    auto name = env->NewStringUTF("dev.duck.toolbox.language.LanguageHooks");
    if (!name) return nullptr;
    auto result = static_cast<jclass>(env->CallObjectMethod(loader, method, name));
    return env->ExceptionCheck() ? nullptr : result;
}
bool install(JNIEnv *env, const std::string &locale, std::vector<char> &dex) {
    if (env->PushLocalFrame(32) < 0) { env->ExceptionClear(); return false; }
    bool resident = false;
    auto setup = [&]() {
        void *art = xdl_open("libart.so", XDL_DEFAULT);
        if (!art) return;
        struct Symbols { void *handle; ~Symbols() { xdl_close(handle); } } symbols{art};
        if (shadowhook_init(SHADOWHOOK_MODE_UNIQUE, false) != 0) return;
        lsplant::InitInfo info{
            .inline_hooker = [](void *target, void *replacement) -> void * {
                void *original = nullptr;
                void *stub = shadowhook_hook_sym_addr(target, replacement, &original);
                if (!stub) return nullptr;
                std::lock_guard lock(trampoline_lock);
                trampolines[target] = stub;
                return original;
            },
            .inline_unhooker = [](void *target) {
                std::lock_guard lock(trampoline_lock);
                auto found = trampolines.find(target);
                if (found == trampolines.end()) return false;
                bool result = shadowhook_unhook(found->second) == 0;
                if (result) trampolines.erase(found);
                return result;
            },
            .art_symbol_resolver = [art](std::string_view symbol) {
                std::string name(symbol);
                void *address = xdl_sym(art, name.c_str(), nullptr);
                return address ? address : xdl_dsym(art, name.c_str(), nullptr);
            },
            .art_symbol_prefix_resolver = [art](std::string_view prefix) {
                std::string name(prefix);
                return duck_xdl_dsym_prefix(art, name.c_str());
            },
            .generated_class_name = "DuckLanguageHook_",
            .generated_source_name = "LanguageOverrides",
        };
        // Initialization itself may install ART hooks; never unload their owner.
        resident = true;
        bool initialized = lsplant::Init(env, info);
        // The pinned LSPlant resolves symbols during InitNative; cached addresses
        // point into ART itself, so xDL's lookup metadata is released on return.
        if (!initialized || env->ExceptionCheck()) {
            __android_log_print(ANDROID_LOG_WARN, "DuckLanguage", "ART language hooks unavailable");
            return;
        }
        install_properties(locale);
        auto entry = load(env, dex);
        if (!entry || env->ExceptionCheck()) return;
        JNINativeMethod methods[] = {{const_cast<char *>("hook"),
            const_cast<char *>("(Ljava/lang/reflect/Method;Ljava/lang/Object;Ljava/lang/reflect/Method;)Ljava/lang/reflect/Method;"),
            reinterpret_cast<void *>(hook)},
            {const_cast<char *>("deoptimize"), const_cast<char *>("(Ljava/lang/reflect/Method;)Z"),
             reinterpret_cast<void *>(deoptimize)}};
        if (env->RegisterNatives(entry, methods, 2) != JNI_OK) return;
        auto method = env->GetStaticMethodID(entry, "install", "(Ljava/lang/String;)I");
        if (!method || env->ExceptionCheck()) return;
        auto tag = env->NewStringUTF(locale.c_str());
        if (!tag || env->ExceptionCheck()) return;
        int count = env->CallStaticIntMethod(entry, method, tag);
        if (!env->ExceptionCheck()) __android_log_print(ANDROID_LOG_INFO, "DuckLanguage", "Installed %d language hooks", count);
    };
    setup();
    if (env->ExceptionCheck()) {
        __android_log_print(ANDROID_LOG_WARN, "DuckLanguage", "Language hook initialization failed");
        env->ExceptionClear();
    }
    env->PopLocalFrame(nullptr);
    return resident;
}
}
