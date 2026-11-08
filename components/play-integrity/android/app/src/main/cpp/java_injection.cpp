/* Derived from Play Integrity Fork, osm0sis, chiteroman and contributors.
 * Source: https://github.com/osm0sis/PlayIntegrityFork/tree/23ec56c2440c4fd1a5e893a23ef7861d6366a828
 * SPDX-License-Identifier: GPL-3.0-only
 * Modified 2026-10-24: independent engine packaging, bounded configuration and runtime operations.
 */
#include "java_injection.hpp"
#include <android/log.h>
#include <charconv>
#include <limits>
#include <strings.h>
#define LOGD(...) __android_log_print(ANDROID_LOG_DEBUG, "PIF/Native", __VA_ARGS__)
#define VENDING_PACKAGE "com.android.vending"
namespace pif {
jstring javaString(JNIEnv *env, std::string_view text) {
    if (text.size() > static_cast<size_t>(std::numeric_limits<jsize>::max()) || env->PushLocalFrame(5) < 0) return nullptr;
    const auto result = [&]() -> jstring {
        auto bytes = env->NewByteArray(static_cast<jsize>(text.size()));
        if (!bytes || env->ExceptionCheck()) return nullptr;
        env->SetByteArrayRegion(bytes, 0, static_cast<jsize>(text.size()),
                                reinterpret_cast<const jbyte *>(text.data()));
        if (env->ExceptionCheck()) return nullptr;
        auto stringClass = env->FindClass("java/lang/String");
        if (!stringClass || env->ExceptionCheck()) return nullptr;
        auto constructor = env->GetMethodID(stringClass, "<init>", "([BLjava/lang/String;)V");
        if (!constructor || env->ExceptionCheck()) return nullptr;
        auto encoding = env->NewStringUTF("UTF-8");
        if (!encoding || env->ExceptionCheck()) return nullptr;
        auto value = static_cast<jstring>(env->NewObject(stringClass, constructor, bytes, encoding));
        return env->ExceptionCheck() ? nullptr : value;
    }();
    return static_cast<jstring>(env->PopLocalFrame(result));
}

static bool setBuildField(JNIEnv *env, jclass type, const std::string &key,
                          const std::string &text, bool allowPrimitives) {
    const auto find = [&](const char *signature) {
        auto field = env->GetStaticFieldID(type, key.c_str(), signature);
        if (env->ExceptionCheck()) { env->ExceptionClear(); return jfieldID{}; }
        return field;
    };
    if (auto field = find("Ljava/lang/String;")) {
        auto value = javaString(env, text);
        if (!env->ExceptionCheck() && value) env->SetStaticObjectField(type, field, value);
        if (!env->ExceptionCheck() && value) env->DeleteLocalRef(value);
        return true;
    }
    if (!allowPrimitives) return false;

    const char *begin = text.c_str();
    if (text.starts_with('+')) ++begin;
    jlong number = 0;
    const auto end = text.c_str() + text.size();
    const auto parsed = std::from_chars(begin, end, number);
    if (parsed.ec == std::errc{} && parsed.ptr == end && !(text.starts_with("+-"))) {
        if (number >= std::numeric_limits<jint>::min() && number <= std::numeric_limits<jint>::max()) {
            if (auto field = find("I")) {
                env->SetStaticIntField(type, field, static_cast<jint>(number));
                return true;
            }
        }
        if (auto field = find("J")) {
            env->SetStaticLongField(type, field, number);
            return true;
        }
    }
    const bool truthy = text == "1" || strcasecmp(text.c_str(), "true") == 0;
    if (truthy || text == "0" || strcasecmp(text.c_str(), "false") == 0) {
        if (auto field = find("Z")) {
            env->SetStaticBooleanField(type, field, truthy ? JNI_TRUE : JNI_FALSE);
            return true;
        }
    }
    return false;
}

void applyBuildFields(JNIEnv *env, const nlohmann::json &build, bool allowPrimitives) {
    if (env->ExceptionCheck()) return;
    if (env->PushLocalFrame(8) < 0) { env->ExceptionClear(); return; }
    const auto writeFields = [&] {
        auto buildClass = env->FindClass("android/os/Build");
        if (env->ExceptionCheck() || !buildClass) return;
        auto versionClass = env->FindClass("android/os/Build$VERSION");
        if (env->ExceptionCheck() || !versionClass) return;
        for (const auto &entry : build.items()) {
            const auto &text = entry.value().get_ref<const std::string &>();
            if (text.empty()) continue;
            if (!setBuildField(env, buildClass, entry.key(), text, allowPrimitives)) {
                setBuildField(env, versionClass, entry.key(), text, allowPrimitives);
            }
            if (env->ExceptionCheck()) return;
        }
    };
    writeFields();
    if (env->ExceptionCheck()) env->ExceptionClear();
    env->PopLocalFrame(nullptr);
}

static void setFieldNative(JNIEnv *env, jclass /* clazz_EntryPoint */, jclass targetClass, jobject fieldObj, jstring typeObj, jobject valueObj) {
    if (!targetClass || !fieldObj || !typeObj) return;

    jfieldID fieldID = env->FromReflectedField(fieldObj);
    if (!fieldID) return;

    const char *typeName = env->GetStringUTFChars(typeObj, nullptr);

    if (!typeName) return;

    if (strcmp(typeName, "java.lang.String") == 0) {
        env->SetStaticObjectField(targetClass, fieldID, valueObj);
    } else if (strcmp(typeName, "int") == 0) {
        jclass intClass = env->FindClass("java/lang/Integer");
        if (!intClass || env->ExceptionCheck()) { env->ReleaseStringUTFChars(typeObj, typeName); return; }
        if (!valueObj || !env->IsInstanceOf(valueObj, intClass)) { env->ReleaseStringUTFChars(typeObj, typeName); return; }
        jmethodID intValue = env->GetMethodID(intClass, "intValue", "()I");
        if (!intValue || env->ExceptionCheck()) { env->ReleaseStringUTFChars(typeObj, typeName); return; }
        jint val = env->CallIntMethod(valueObj, intValue);
        if (env->ExceptionCheck()) { env->ReleaseStringUTFChars(typeObj, typeName); return; }
        env->SetStaticIntField(targetClass, fieldID, val);
    } else if (strcmp(typeName, "long") == 0) {
        jclass longClass = env->FindClass("java/lang/Long");
        if (!longClass || env->ExceptionCheck()) { env->ReleaseStringUTFChars(typeObj, typeName); return; }
        if (!valueObj || !env->IsInstanceOf(valueObj, longClass)) { env->ReleaseStringUTFChars(typeObj, typeName); return; }
        jmethodID longValue = env->GetMethodID(longClass, "longValue", "()J");
        if (!longValue || env->ExceptionCheck()) { env->ReleaseStringUTFChars(typeObj, typeName); return; }
        jlong val = env->CallLongMethod(valueObj, longValue);
        if (env->ExceptionCheck()) { env->ReleaseStringUTFChars(typeObj, typeName); return; }
        env->SetStaticLongField(targetClass, fieldID, val);
    } else if (strcmp(typeName, "boolean") == 0) {
        jclass boolClass = env->FindClass("java/lang/Boolean");
        if (!boolClass || env->ExceptionCheck()) { env->ReleaseStringUTFChars(typeObj, typeName); return; }
        if (!valueObj || !env->IsInstanceOf(valueObj, boolClass)) { env->ReleaseStringUTFChars(typeObj, typeName); return; }
        jmethodID booleanValue = env->GetMethodID(boolClass, "booleanValue", "()Z");
        if (!booleanValue || env->ExceptionCheck()) { env->ReleaseStringUTFChars(typeObj, typeName); return; }
        jboolean val = env->CallBooleanMethod(valueObj, booleanValue);
        if (env->ExceptionCheck()) { env->ReleaseStringUTFChars(typeObj, typeName); return; }
        env->SetStaticBooleanField(targetClass, fieldID, val);
    }

    env->ReleaseStringUTFChars(typeObj, typeName);
}

void JavaInjection::initialize(JNIEnv *environment, std::vector<char> dex, const Settings &settings) {
    if (settings.nativeBuild) applyBuildFields(environment, settings.build, !settings.vending);
    if (!settings.injectDex()) return;
    env=environment; dexVector=std::move(dex); json=settings.build;
    pkgName=settings.vending ? VENDING_PACKAGE : "com.google.android.gms.unstable";
    vendingFingerprintValue=settings.vendingFingerprint;
    verboseLogs=settings.verboseLogs; spoofBuild=settings.nativeBuild ? 0 : settings.spoofBuild;
    spoofProvider=settings.spoofProvider; spoofSignature=settings.spoofSignature;
    spoofVendingFinger=settings.spoofVendingFinger; spoofVendingSdk=settings.spoofVendingSdk;
    inject();
}
bool JavaInjection::failed(const char *operation) {
        if (!env->ExceptionCheck()) return false;
        LOGD("JNI failed: %s", operation); env->ExceptionDescribe(); env->ExceptionClear(); return true;
    }
void JavaInjection::inject() {
        if (env->PushLocalFrame(32) < 0) { env->ExceptionClear(); return; }
        injectFrame();
        if (env->ExceptionCheck()) { LOGD("DEX injection failed"); env->ExceptionDescribe(); env->ExceptionClear(); }
        env->PopLocalFrame(nullptr);
    }
void JavaInjection::injectFrame() {
        auto clClass = env->FindClass("java/lang/ClassLoader");
        if (!clClass || failed("ClassLoader")) return;
        auto getSystem = env->GetStaticMethodID(clClass, "getSystemClassLoader", "()Ljava/lang/ClassLoader;");
        if (!getSystem || failed("getSystemClassLoader")) return;
        auto systemLoader = env->CallStaticObjectMethod(clClass, getSystem);
        if (!systemLoader || failed("system classloader")) return;
        auto dexClass = env->FindClass("dalvik/system/InMemoryDexClassLoader");
        if (!dexClass || failed("InMemoryDexClassLoader")) return;
        auto init = env->GetMethodID(dexClass, "<init>", "(Ljava/nio/ByteBuffer;Ljava/lang/ClassLoader;)V");
        if (!init || failed("DEX constructor")) return;
        auto buffer = env->NewDirectByteBuffer(dexVector.data(), static_cast<jlong>(dexVector.size()));
        if (!buffer || failed("DEX buffer")) return;
        auto localLoader = env->NewObject(dexClass, init, buffer, systemLoader);
        if (!localLoader || failed("DEX loader")) return;
        loader = env->NewGlobalRef(localLoader);
        if (!loader || failed("loader reference")) return;
        auto load = env->GetMethodID(clClass, "loadClass", "(Ljava/lang/String;)Ljava/lang/Class;");
        if (!load || failed("loadClass method")) return;
        const bool vending = pkgName == VENDING_PACKAGE;
        auto className = env->NewStringUTF(vending ? "es.chiteroman.playintegrityfix.EntryPointVending" : "es.chiteroman.playintegrityfix.EntryPoint");
        if (!className || failed("entry name")) return;
        auto entry = static_cast<jclass>(env->CallObjectMethod(loader, load, className));
        if (!entry || failed("entry class")) return;
        JNINativeMethod methods[] = {{const_cast<char *>("setFieldNative"), const_cast<char *>("(Ljava/lang/Class;Ljava/lang/reflect/Field;Ljava/lang/String;Ljava/lang/Object;)V"), reinterpret_cast<void *>(setFieldNative)}};
        if (env->RegisterNatives(entry, methods, 1) != JNI_OK || failed("RegisterNatives")) return;
        if (vending) {
            auto method = env->GetStaticMethodID(entry, "init", "(IIILjava/lang/String;)V");
            if (!method || failed("vending init")) return;
            auto value = javaString(env, vendingFingerprintValue);
            if (!value || failed("fingerprint")) return;
            env->CallStaticVoidMethod(entry, method, verboseLogs, spoofVendingFinger, spoofVendingSdk, value);
        } else {
            auto receive = env->GetStaticMethodID(entry, "receiveJson", "(Ljava/lang/String;)V");
            if (!receive || failed("receiveJson")) return;
            auto value = env->NewStringUTF(json.dump(-1, ' ', true).c_str());
            if (!value || failed("config string")) return;
            env->CallStaticVoidMethod(entry, receive, value);
            if (failed("receiveJson call")) return;
            auto method = env->GetStaticMethodID(entry, "init", "(IIII)V");
            if (!method || failed("entry init")) return;
            env->CallStaticVoidMethod(entry, method, verboseLogs, spoofBuild, spoofProvider, spoofSignature);
        }
        if (!failed("entry init call")) LOGD("Injection initialized for %s", pkgName.c_str());
    }

}
