// SPDX-License-Identifier: GPL-3.0-only
// Exercise the production UTF-8 boundary with a real host JVM and checked JNI calls.
#include "java_injection.hpp"
#include "observation.hpp"
#include "private_properties.hpp"
#include "transport.hpp"
#include <algorithm>
#include <cassert>
#include <cstring>
#include <string>
#include <sys/system_properties.h>

static std::string observation;
static std::string propertyValue="readback";
struct prop_info { std::string name, callback, direct; };
static std::vector<prop_info> fixture;
static bool patchAvailable = true;
static int patchCalls = 0, scans = 0;
int __android_log_print(int, const char *, const char *, ...) { return 0; }
int __android_log_write(int, const char *tag, const char *message) {
    if (std::string_view(tag)=="PIF/Observation") observation=message;
    return 0;
}
const prop_info *__system_property_find(const char *name) {
    if (fixture.empty()) return reinterpret_cast<const prop_info *>(1);
    auto property = std::ranges::find(fixture, name, &prop_info::name);
    return property == fixture.end() ? nullptr : &*property;
}
void __system_property_read_callback(const prop_info *property,void (*callback)(void *,const char *,const char *,uint32_t),void *cookie) {
    if (fixture.empty()) callback(cookie,"property",propertyValue.c_str(),1);
    else callback(cookie,property->name.c_str(),property->callback.c_str(),1);
}
int __system_property_get(const char *name, char *value) {
    auto property = __system_property_find(name);
    if (!property) return 0;
    assert(property->direct.size() < PROP_VALUE_MAX);
    std::strcpy(value, property->direct.c_str());
    return static_cast<int>(property->direct.size());
}
int __system_property_foreach(void (*visit)(const prop_info *,void *),void *cookie) {
    ++scans;
    for (const auto &property : fixture) visit(&property,cookie);
    return 0;
}
struct Property { const char *name; const char *value; };
extern "C" bool duck_faker_patch_existing(const Property *entries, size_t count) {
    ++patchCalls;
    if (!patchAvailable) return false;
    for (size_t i = 0; i < count; ++i) {
        auto property = std::ranges::find(fixture, entries[i].name, &prop_info::name);
        assert(property != fixture.end());
        property->callback = property->direct = entries[i].value;
    }
    return true;
}

static void privatePropertyTests() {
    fixture = {{"ro.product.model","Exact","Original"},{"ro.vendor.model","Vendor","Vendor"}};
    pif::Settings settings;
    settings.properties = {{"ro.product.model","Exact"},{"ro.missing.model","Missing"}};
    settings.privateDefaults = {{"ro.product.model","Baseline"}};
    pif::PrivateProperties exact;
    assert(exact.prepare(settings) && exact.size()==1 && exact.matching()==1);
    assert(patchCalls==1 && scans==0);
    pif::PrivateProperties unchanged;
    assert(unchanged.prepare(settings) && unchanged.matching()==1 && patchCalls==1);

    settings.properties.emplace("*.model","Suffix");
    settings.privateDefaults.emplace("ro.vendor.model","Baseline");
    patchAvailable = false;
    pif::PrivateProperties unavailable;
    assert(!unavailable.prepare(settings) && unavailable.size()==0);
    patchAvailable = true;
    pif::PrivateProperties wildcard;
    assert(wildcard.prepare(settings) && wildcard.size()==2 && wildcard.matching()==2);
    assert(fixture[0].direct=="Exact" && fixture[1].direct=="Suffix" && scans==2);
    fixture.clear();
}

static void check(JNIEnv *env, std::string_view input, std::u16string_view expected) {
    auto value = pif::javaString(env, input);
    assert(value && !env->ExceptionCheck());
    assert(env->GetStringLength(value) == static_cast<jsize>(expected.size()));
    const auto *characters = env->GetStringChars(value, nullptr);
    assert(characters && !env->ExceptionCheck());
    for (size_t i = 0; i < expected.size(); ++i) assert(characters[i] == expected[i]);
    env->ReleaseStringChars(value, characters);
    env->DeleteLocalRef(value);
}

int main(int argc,char **argv) {
    assert(argc==2);
    privatePropertyTests();
    JavaVM *vm = nullptr;
    JNIEnv *env = nullptr;
    std::string classpath=std::string("-Djava.class.path=")+argv[1];
    JavaVMOption options[]={{const_cast<char *>("-Xcheck:jni"),nullptr},{classpath.data(),nullptr}};
    JavaVMInitArgs args{};
    args.version = JNI_VERSION_1_8;
    args.nOptions = 2;
    args.options = options;
    assert(JNI_CreateJavaVM(&vm, reinterpret_cast<void **>(&env), &args) == JNI_OK);
    check(env, "", u"");
    check(env, "google/device:16/BUILD:user/release-keys", u"google/device:16/BUILD:user/release-keys");
    check(env, "\xe4\xb8\xad\xe6\x96\x87", u"\u4e2d\u6587");
    check(env, "A\xf0\x9f\x98\x80Z", u"A\U0001f600Z");
    check(env, std::string("a\0b", 3), std::u16string(u"a\0b", 3));
    for (int i = 0; i < 1000; ++i) check(env, "\xf0\x9f\x98\x80", u"\U0001f600");
    pif::observe(env,"abc","com.android.vending","boot","start",true);
    assert(!env->ExceptionCheck());
    auto record=nlohmann::json::parse(observation);
    assert(record["revision"]=="ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    assert(record["build"]["MODEL"]=="Pixel \xf0\x9f\x98\x80");
    assert(record["build"]["SDK_INT"]=="36");
    assert(record["properties"]["ro.product.model"]=="readback");
    pif::applyBuildFields(env,{{"MODEL","Native \xf0\x9f\x98\x80"},{"SECURITY_PATCH","2026-11-05"},{"SDK_INT","32"},{"UNKNOWN","unused"}},false);
    pif::observe(env,"abc","com.android.vending","boot","start",false,3,2);
    record=nlohmann::json::parse(observation);
    assert(record["build"]["MODEL"]=="Native \xf0\x9f\x98\x80");
    assert(record["build"]["SECURITY_PATCH"]=="2026-11-05");
    assert(record["build"]["SDK_INT"]=="36" && !env->ExceptionCheck());
    assert(record["private_requested"]==3 && record["private_matching"]==2);
    pif::applyBuildFields(env,{{"SDK_INT","+33"},{"TIME","2147483648"},{"IS_DEBUGGABLE","TrUe"}},true);
    pif::observe(env,"abc","com.google.android.gms.unstable","boot","start",false);
    assert(nlohmann::json::parse(observation)["build"]["SDK_INT"]=="33");
    auto build=env->FindClass("android/os/Build");
    assert(!env->ExceptionCheck() && build);
    auto time=env->GetStaticFieldID(build,"TIME","J");
    assert(!env->ExceptionCheck() && time && env->GetStaticLongField(build,time)==2147483648LL);
    auto debug=env->GetStaticFieldID(build,"IS_DEBUGGABLE","Z");
    assert(!env->ExceptionCheck() && debug && env->GetStaticBooleanField(build,debug)==JNI_TRUE);
    env->DeleteLocalRef(build);
    propertyValue=std::string(140,'a');
    pif::observe(env,"abc","com.android.vending","boot","start",false,3,2);
    assert(observation.size()>1023 && nlohmann::json::parse(observation)["private_matching"]==2);
    propertyValue=std::string(256,'\x01');
    pif::observe(env,"abc","com.android.vending","boot","start",false,3,2);
    assert(observation.size()<3500 && nlohmann::json::parse(observation)["private_matching"]==2);
    propertyValue="\xff";
    for(int i=0;i<100;++i) {
        pif::observe(env,"abc","com.android.vending","boot","start",false);
        assert(!env->ExceptionCheck());
        assert(nlohmann::json::parse(observation)["properties"].empty());
    }
    propertyValue=std::string(257,'a');
    pif::observe(env,"abc","com.android.vending","boot","start",false);
    assert(nlohmann::json::parse(observation)["properties"].empty());
    assert(vm->DestroyJavaVM() == JNI_OK);
}
