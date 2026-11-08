// SPDX-License-Identifier: GPL-3.0-only
// Independent native parser and companion protocol verification.
#include "transport.hpp"
#include "settings.hpp"
#include <cassert>
#include <atomic>
#include <filesystem>
#include <fstream>
#include <thread>
#include <sys/socket.h>
#include <unistd.h>
static void closePair(int (&fd)[2]) { close(fd[0]); close(fd[1]); }
static void deadlineTests() {
    using namespace std::chrono;
    int fd[2]; assert(socketpair(AF_UNIX,SOCK_STREAM,0,fd)==0);
    std::atomic<bool> stop{false};
    std::thread dribble([&] {
        uint32_t header[]{pif::magic,1,2};
        const auto *bytes=reinterpret_cast<const char *>(header);
        for(size_t i=0;i<sizeof(header) && !stop;++i) {
            if(send(fd[1],bytes+i,1,MSG_NOSIGNAL)!=1) return;
            std::this_thread::sleep_for(milliseconds(210));
        }
        if(!stop) send(fd[1],"d{}",3,MSG_NOSIGNAL);
    });
    std::vector<char> dex{'s'};std::string config="stale";
    auto start=steady_clock::now();
    bool received=pif::receive(fd[0],dex,config);
    const auto elapsed=steady_clock::now()-start;
    stop=true;shutdown(fd[0],SHUT_RDWR);dribble.join();closePair(fd);
    assert(!received && dex.empty() && config.empty());
    assert(elapsed < seconds(4));

    assert(socketpair(AF_UNIX,SOCK_STREAM,0,fd)==0);
    std::vector<char> payload(1024*1024,'x');
    start=steady_clock::now();
    assert(!pif::transfer(fd[0],payload.data(),payload.size(),true,start+milliseconds(100)));
    assert(steady_clock::now()-start < seconds(1));closePair(fd);

    // The same deadline applies across separate pieces of a protocol frame.
    assert(socketpair(AF_UNIX,SOCK_STREAM,0,fd)==0);
    auto deadline=steady_clock::now()+milliseconds(100);
    char value='x';assert(write(fd[1],&value,1)==1);
    assert(pif::transfer(fd[0],&value,1,false,deadline));
    std::this_thread::sleep_for(milliseconds(120));
    assert(write(fd[1],&value,1)==1);
    assert(!pif::transfer(fd[0],&value,1,false,deadline));closePair(fd);
}
#ifdef PIF_TEST_ROOT
static void gateTests() {
    namespace fs=std::filesystem;
    const auto root=fs::path(PIF_TEST_ROOT);
    fs::create_directories(root / "module/play-integrity"); fs::create_directories(root / "data");
    const auto write=[&](const char *path,const char *value){std::ofstream(root / path)<<value;};
    write("module/play-integrity/classes.dex","dex"); write("data/custom.pif.json","{\"MODEL\":\"A\"}");
    const auto received=[](bool expectDex=false){int fd[2];assert(socketpair(AF_UNIX,SOCK_STREAM,0,fd)==0);std::thread server([&]{pif::companion(fd[1]);close(fd[1]);});std::vector<char> dex;std::string config;const bool result=pif::receive(fd[0],dex,config);close(fd[0]);server.join();if(result) assert(expectDex==!dex.empty());return result;};
    assert(!received());
    for (const auto *options : {"", "{}", "{\"mode\":\"full\"}", "{\"enabled\":null}", "{\"enabled\":\"true\"}", "{\"enabled\":1}"}) {
        write("data/options.json",options); assert(!received());
    }
    write("data/options.json","{\"enabled\":false}"); assert(!received());
    write("data/options.json","{\"enabled\":true}"); assert(received());
    write("data/custom.pif.json","{\"MODEL\":\"A\",\"fullInjection\":\"0\"}"); assert(received(true));
    write("data/custom.pif.json","{\"MODEL\":\"A\",\"spoofProvider\":\"1\"}"); assert(received(true));
    write("data/custom.pif.json","{\"MODEL\":\"A\"}"); assert(received());
    for (const auto *key : {"skipdelprop", "skippersistprop", "skipmatchdevice"}) {
        std::ofstream(root / "data/options.json") << "{\"enabled\":true,\"" << key << "\":\"false\"}";
        assert(!received());
        std::ofstream(root / "data/options.json") << "{\"enabled\":true,\"" << key << "\":false}";
        assert(received());
    }
    write("data/disabled",""); write("data/enabled-token",""); assert(!received());
    write("data/disabled","first-stop"); assert(!received());
    write("data/enabled-token","first-stop"); assert(received());
    write("data/disabled","later-stop"); assert(!received());
    write("data/enabled-token","later-stop"); assert(received());
    write("data/options.json","{broken"); assert(!received());
    write("data/options.json","{\"enabled\":true,\"mode\":\"scripts-only\"}"); assert(!received());
    write("data/options.json","{\"enabled\":true}"); write("module/disable",""); assert(!received());
    fs::remove(root / "module/disable"); write("data/custom.pif.json",""); assert(!received());
}
#endif
int main() {
    deadlineTests();
#ifdef PIF_TEST_ROOT
    gateTests();
#endif
    auto parsed = pif::parse("MODEL=with \"quotes\"\n*.security_patch=2026-10-05");
    assert(parsed["MODEL"] == "with \"quotes\"");
    assert(parsed["*.security_patch"] == "2026-10-05");
    assert(pif::parse("{\"MODEL\":[]}").empty());
    assert(pif::parse("{broken").empty());
    assert(pif::parse("MODEL=\xff").empty());
    assert(pif::parse("\xff=bad").empty());
    assert(pif::parse("MODEL=A\xf0\x9f\x98\x80Z")["MODEL"] == "A\xf0\x9f\x98\x80Z");
    auto scalars = pif::parse("{\"spoofBuild\":true,\"SDK_INT\":36,\"EXTRA\":null}");
    assert(scalars["spoofBuild"] == "1" && scalars["SDK_INT"] == "36" && scalars["EXTRA"] == "");
    assert(pif::parse("{\"MODEL\":\"bad\\nvalue\"}").empty());
    assert(pif::number("2147483648", 0) == 0);
    assert(pif::number("1tail", 0) == 0);
    assert(pif::number("100", 0) == 100);
    auto full = pif::parse("FINGERPRINT=google/rango_beta/rango:CANARY/ID/123:user/release-keys\nMODEL=Phone\nDEVICE_INITIAL_SDK_INT=36\nspoofVendingBuild=true\nspoofVendingSdk=true");
    auto store = pif::settings(full,true);
    assert(store.nativeBuild && !store.spoofProps && !store.injectDex());
    assert(store.build["MODEL"] == "Phone" && store.build["RELEASE"] == "CANARY");
    assert(!store.spoofVendingSdk);
    auto gms = pif::settings(full,false);
    assert(gms.nativeBuild && !gms.injectDex());
    assert(gms.privateDefaults.at("ro.product.first_api_level") == "36");
    assert(gms.privateDefaults.at("ro.board.first_api_level") == "32");
    assert(pif::settings(pif::parse("MODEL=Phone"),true).nativeBuild);
    assert(pif::settings(pif::parse("MODEL=Phone\nspoofVendingSdk=1"),true).spoofVendingSdk == 1);
    assert(!pif::settings(pif::parse("MODEL=Phone\nspoofVendingBuild=0"),true).injectJava());
    auto legacy = pif::settings(pif::parse("MODEL=Phone\nfullInjection=0"),false);
    assert(!legacy.nativeBuild && legacy.injectDex() && legacy.privateDefaults.empty());
    int fd[2]; assert(socketpair(AF_UNIX, SOCK_STREAM, 0, fd) == 0);
    std::thread writer([&] {
        uint32_t header[]{pif::magic, 3, 2};
        auto *bytes = reinterpret_cast<char *>(header);
        for (size_t i = 0; i < sizeof(header); ++i) assert(write(fd[1], bytes+i, 1) == 1);
        assert(write(fd[1], "dex{}", 5) == 5);
    });
    std::vector<char> dex; std::string config;
    assert(pif::receive(fd[0], dex, config)); assert(dex.size() == 3 && config == "{}");
    writer.join(); closePair(fd);
    assert(socketpair(AF_UNIX, SOCK_STREAM, 0, fd) == 0);
    uint32_t tooLarge[]{pif::magic, pif::max_dex+1, 1};
    assert(write(fd[1], tooLarge, sizeof(tooLarge)) == sizeof(tooLarge));
    assert(!pif::receive(fd[0], dex, config)); closePair(fd);
    assert(socketpair(AF_UNIX, SOCK_STREAM, 0, fd) == 0);
    assert(write(fd[1], "bad", 3) == 3); close(fd[1]);
    assert(!pif::receive(fd[0], dex, config)); close(fd[0]);
}
