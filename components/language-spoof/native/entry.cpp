// SPDX-License-Identifier: GPL-3.0-only
#include "language.hpp"
#include "zygisk.hpp"
#include <unistd.h>

namespace duck::language {
class Module : public zygisk::ModuleBase {
    zygisk::Api *api = nullptr;
    JNIEnv *env = nullptr;
    std::string locale;
    std::vector<char> dex;
    void unload() { api->setOption(zygisk::DLCLOSE_MODULE_LIBRARY); }
    std::string string(jstring value) {
        if (!value) return {};
        const char *text = env->GetStringUTFChars(value, nullptr);
        if (!text) { env->ExceptionClear(); return {}; }
        std::string result(text);
        env->ReleaseStringUTFChars(value, text);
        return result;
    }
public:
    void onLoad(zygisk::Api *platform, JNIEnv *environment) override { api = platform; env = environment; }
    void preAppSpecialize(zygisk::AppSpecializeArgs *args) override {
        Target selected;
        if (!target(static_cast<unsigned>(args->uid), string(args->nice_name), string(args->app_data_dir), selected)) { unload(); return; }
        int fd = api->connectCompanion();
        bool ready = fd >= 0 && exchange(fd, selected, locale, dex);
        if (fd >= 0) close(fd);
        if (!ready) { locale.clear(); dex.clear(); unload(); }
    }
    void postAppSpecialize(const zygisk::AppSpecializeArgs *) override {
        if (locale.empty()) return;
        // After ART installs any callbacks the shared ELF must remain resident,
        // including on a partial setup failure. The process owns the DEX buffer.
        if (!install(env, locale, dex)) unload();
    }
    void preServerSpecialize(zygisk::ServerSpecializeArgs *) override { unload(); }
};
}
extern "C" void duck_language_module_entry(zygisk::internal::api_table *table, JNIEnv *env) {
    zygisk::internal::entry_impl<duck::language::Module>(table, env);
}
extern "C" void duck_language_companion(int fd) { duck::language::companion(fd); }
