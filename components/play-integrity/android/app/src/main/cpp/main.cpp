/* Derived from Play Integrity Fork, osm0sis, chiteroman and contributors.
 * Source: https://github.com/osm0sis/PlayIntegrityFork/tree/23ec56c2440c4fd1a5e893a23ef7861d6366a828
 * SPDX-License-Identifier: GPL-3.0-only
 * Modified 2026-10-23: independent engine packaging, bounded configuration and runtime operations.
 */
#include <android/log.h>
#include <unistd.h>
#include "zygisk.hpp"
#include "transport.hpp"
#include "settings.hpp"
#include "property_hook.hpp"
#include "java_injection.hpp"
#include "observation.hpp"
#define LOGD(...) __android_log_print(ANDROID_LOG_DEBUG, "PIF/Native", __VA_ARGS__)

class PlayIntegrityFix : public zygisk::ModuleBase {
public:
    void onLoad(zygisk::Api *platform,JNIEnv *environment) override {api=platform;env=environment;}
    void preAppSpecialize(zygisk::AppSpecializeArgs *args) override {
        if(!args->nice_name || !args->app_data_dir) {unload();return;}
        const char *name=env->GetStringUTFChars(args->nice_name,nullptr);
        if(!name) {env->ExceptionClear();unload();return;}
        process=name;env->ReleaseStringUTFChars(args->nice_name,name);
        const char *path=env->GetStringUTFChars(args->app_data_dir,nullptr);
        if(!path) {env->ExceptionClear();unload();return;}
        std::string_view directory(path);
        const bool google=directory.ends_with("/com.google.android.gms")||directory.ends_with("/com.android.vending");
        env->ReleaseStringUTFChars(args->app_data_dir,path);
        if(!google) {unload();return;}
        int fd=api->connectCompanion();std::string content;
        bool received=fd>=0 && pif::receive(fd,dex,content);
        if(fd>=0) close(fd);
        if(!received) {LOGD("Engine inactive or companion unavailable");unload();return;}
        auto config=pif::parse(content);
        if(!config.is_object()||config.empty()) {unload();return;}
        api->setOption(zygisk::FORCE_DENYLIST_UNMOUNT);
        if(process!="com.google.android.gms.unstable" && process!="com.android.vending") {unload();return;}
        raw=std::move(content);boot=pif::bootIdentity();start=pif::processStart();
        settings=pif::settings(std::move(config),process=="com.android.vending");ready=true;
    }
    void postAppSpecialize(const zygisk::AppSpecializeArgs *) override {
        if(!ready) return;
        const bool propertyHook=settings.spoofProps && hooks.install(settings);
        if(settings.injectJava()) java.initialize(env,std::move(dex),settings);
        pif::observe(env,raw,process,boot,start,propertyHook);
        std::string().swap(raw);
        LOGD("Specialization initialized for %s",process.c_str());
    }
    void preServerSpecialize(zygisk::ServerSpecializeArgs *) override {unload();}
private:
    void unload() {api->setOption(zygisk::DLCLOSE_MODULE_LIBRARY);}
    zygisk::Api *api=nullptr;
    JNIEnv *env=nullptr;
    bool ready=false;
    std::string process;
    std::string raw,boot,start;
    std::vector<char> dex;
    pif::Settings settings;
    pif::PropertyHook hooks;
    pif::JavaInjection java;
};
#ifdef DUCK_EMBEDDED_ENGINES
extern "C" void duck_pif_module_entry(zygisk::internal::api_table *table, JNIEnv *env) {
    zygisk::internal::entry_impl<PlayIntegrityFix>(table, env);
}
extern "C" void duck_pif_companion(int fd) { pif::companion(fd); }
#else
REGISTER_ZYGISK_MODULE(PlayIntegrityFix)
REGISTER_ZYGISK_COMPANION(pif::companion)
#endif
