// SPDX-License-Identifier: GPL-3.0-only
// One bounded, process-local readback. Log records are diagnostic data, never verdicts.
#include "observation.hpp"
#include "transport.hpp"
#include <android/log.h>
#include <fstream>
#include <sstream>
#include <cstring>
#include <chrono>
#include <sys/system_properties.h>
#include <unistd.h>
namespace pif {
static std::string line(const char *path) {
    std::ifstream file(path); std::string value; std::getline(file,value);
    return value.size() <= 4096 ? value : std::string();
}
std::string bootIdentity() { return line("/proc/sys/kernel/random/boot_id"); }
std::string processStart() {
    auto value=line("/proc/self/stat");auto end=value.rfind(')');
    if(end==std::string::npos) return {};
    std::istringstream fields(value.substr(end+1));std::string field;
    for(int i=0;i<=19;++i) if(!(fields>>field)) return {};
    return field;
}
static bool error(JNIEnv *env) {
    if(!env->ExceptionCheck()) return false;
    env->ExceptionClear(); return true;
}
static std::string utf8(JNIEnv *env,jstring value) {
    if(!value) return {};
    auto type=env->FindClass("java/lang/String");
    if(error(env)||!type) return {};
    auto method=env->GetMethodID(type,"getBytes","(Ljava/lang/String;)[B");
    if(error(env)||!method) return {};
    auto encoding=env->NewStringUTF("UTF-8");
    if(error(env)||!encoding) return {};
    auto bytes=static_cast<jbyteArray>(env->CallObjectMethod(value,method,encoding));
    if(error(env)||!bytes) return {};
    const auto size=env->GetArrayLength(bytes);
    if(size>256) return {};
    std::string result(static_cast<size_t>(size),'\0');
    env->GetByteArrayRegion(bytes,0,size,reinterpret_cast<jbyte *>(result.data()));
    if(error(env)) return {};
    return result;
}
static std::string digest(JNIEnv *env,const std::string &raw) {
    auto type=env->FindClass("java/security/MessageDigest");
    if(error(env)||!type) return {};
    auto get=env->GetStaticMethodID(type,"getInstance","(Ljava/lang/String;)Ljava/security/MessageDigest;");
    if(error(env)||!get) return {};
    auto name=env->NewStringUTF("SHA-256");
    if(error(env)||!name) return {};
    auto instance=env->CallStaticObjectMethod(type,get,name);
    if(error(env)||!instance) return {};
    auto method=env->GetMethodID(type,"digest","([B)[B");
    if(error(env)||!method) return {};
    auto input=env->NewByteArray(static_cast<jsize>(raw.size()));
    if(error(env)||!input) return {};
    env->SetByteArrayRegion(input,0,static_cast<jsize>(raw.size()),reinterpret_cast<const jbyte *>(raw.data()));
    if(error(env)) return {};
    auto output=static_cast<jbyteArray>(env->CallObjectMethod(instance,method,input));
    if(error(env)||!output||env->GetArrayLength(output)!=32) return {};
    unsigned char bytes[32]; env->GetByteArrayRegion(output,0,32,reinterpret_cast<jbyte *>(bytes));
    if(error(env)) return {};
    const char *hex="0123456789abcdef";std::string result;
    for(auto byte:bytes) {result+=hex[byte>>4];result+=hex[byte&15];}
    return result;
}
static void includeField(nlohmann::json &fields, size_t &remaining, const char *key, std::string value) {
    if (value.empty()) return;
    const auto encoded = nlohmann::json(value).dump(-1, ' ', false, nlohmann::json::error_handler_t::replace);
    const auto size = strlen(key) + 4 + encoded.size();
    if (size > remaining || nlohmann::json::parse(encoded) != value) return;
    remaining -= size;
    fields[key] = std::move(value);
}

void observe(JNIEnv *env,const std::string &raw,const std::string &process,
             const std::string &boot,const std::string &start,bool propertyHook,
             size_t privateRequested,size_t privateMatching) {
    if(raw.size()>max_config||env->PushLocalFrame(128)<0) {env->ExceptionClear();return;}
    const auto revision=digest(env,raw);
    nlohmann::json build=nlohmann::json::object(),props=nlohmann::json::object();
    // Give each readback group its own budget so long fields cannot hide the
    // process identity or private-property result from the diagnostics.
    size_t buildBudget=1400, propertyBudget=1400;
    for(const auto *key:{"SDK_INT","DEVICE_INITIAL_SDK_INT"}) {
        auto type=env->FindClass("android/os/Build$VERSION");
        if(error(env)||!type) continue;
        auto field=env->GetStaticFieldID(type,key,"I");
        if(error(env)||!field) {env->DeleteLocalRef(type);continue;}
        const auto value=env->GetStaticIntField(type,field);
        if(!error(env)) includeField(build,buildBudget,key,std::to_string(value));
        env->DeleteLocalRef(type);
    }
    for(const auto *key:{"MODEL","FINGERPRINT","SECURITY_PATCH","BRAND","MANUFACTURER","PRODUCT","DEVICE","ID","RELEASE","INCREMENTAL","TYPE","TAGS"}) {
        const bool version=std::string_view(key)=="RELEASE"||std::string_view(key)=="SECURITY_PATCH";
        auto type=env->FindClass(version?"android/os/Build$VERSION":"android/os/Build");
        if(error(env)||!type) continue;
        auto field=env->GetStaticFieldID(type,key,"Ljava/lang/String;");
        if(error(env)||!field) continue;
        auto value=static_cast<jstring>(env->GetStaticObjectField(type,field));
        if(error(env)) continue;
        includeField(build,buildBudget,key,utf8(env,value));
        env->DeleteLocalRef(type); if(value) env->DeleteLocalRef(value);
    }
    for(const auto *key:{"ro.product.model","ro.build.fingerprint","ro.product.first_api_level","ro.build.version.security_patch","ro.product.brand","ro.product.manufacturer","ro.product.name","ro.product.device","ro.build.id","ro.build.version.release","ro.build.version.incremental","ro.build.type","ro.build.tags"}) {
        if(const auto *info=__system_property_find(key)) {
            std::string value;
            __system_property_read_callback(info,[](void *cookie,const char *,const char *text,uint32_t) {
                if(text && strlen(text)<=256) *static_cast<std::string *>(cookie)=text;
            },&value);
            includeField(props,propertyBudget,key,std::move(value));
        }
    }
    auto record=nlohmann::json({{"revision",revision},{"boot_id",boot},{"start",start},{"pid",getpid()},
        {"process",process},{"build",build},{"properties",props},{"property_hook",propertyHook},
        {"private_requested",privateRequested},{"private_matching",privateMatching},
        {"observed_at",std::chrono::duration_cast<std::chrono::seconds>(std::chrono::system_clock::now().time_since_epoch()).count()}}).dump();
    if(revision.size()==64 && !boot.empty() && !start.empty() && record.size()<3500)
        __android_log_write(ANDROID_LOG_INFO,"PIF/Observation",record.c_str());
    env->PopLocalFrame(nullptr);
}
}
