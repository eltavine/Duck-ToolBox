// SPDX-License-Identifier: GPL-3.0-only
// Derived from Play Integrity Fork (23ec56c); modified 2026-10-23: independent engine build.
plugins {
    id("com.android.application")
}

android {
    namespace = "es.chiteroman.playintegrityfix"
    compileSdk = 37
    ndkVersion = "30.0.16248370"
    buildToolsVersion = "37.0.0"

    buildFeatures {
        prefab = true
    }

    packaging {
        jniLibs {
            excludes += "**/liblog.so"
        }
    }

    defaultConfig {
        applicationId = "es.chiteroman.playintegrityfix"
        minSdk = 26
        targetSdk = 37
        versionCode = 1
        versionName = "1.0"

        ndk {
            abiFilters += "armeabi-v7a"
            abiFilters += "arm64-v8a"
        }

        externalNativeBuild {
            cmake {
                arguments += "-DANDROID_STL=none"
                arguments += "-DPIF_VENDOR_DIR=${System.getenv("PIF_VENDOR_DIR") ?: ""}"
                arguments += "-DCMAKE_BUILD_TYPE=Release"
                arguments += "-DDUCK_RUNTIME_DIR=${System.getenv("DUCK_RUNTIME_DIR") ?: ""}"
                arguments += "-DDUCK_LANGUAGE_DIR=${System.getenv("DUCK_LANGUAGE_DIR") ?: ""}"
                arguments += "-DDUCK_FAKER_LIBRARY=${System.getenv("DUCK_FAKER_LIBRARY") ?: ""}"

                cppFlags += "-std=c++23"
                cppFlags += "-fno-exceptions"
                cppFlags += "-fno-rtti"
                cppFlags += "-fvisibility=hidden"
                cppFlags += "-fvisibility-inlines-hidden"
            }
        }
    }

    System.getenv("DUCK_FAKER_JAVA")?.takeIf { it.isNotBlank() }?.let { source ->
        sourceSets.getByName("main").java.srcDir(source)
    }
    System.getenv("DUCK_LANGUAGE_DIR")?.takeIf { it.isNotBlank() }?.let { source ->
        sourceSets.getByName("main").java.srcDir("$source/java")
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            System.getenv("DUCK_LANGUAGE_DIR")?.takeIf { it.isNotBlank() }?.let { source ->
                proguardFiles("$source/proguard-rules.pro")
            }
            System.getenv("DUCK_FAKER_JAVA")?.takeIf { it.isNotBlank() }?.let { source ->
                proguardFiles(file(source).resolve("../proguard-rules.pro"))
            }
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_21
        targetCompatibility = JavaVersion.VERSION_21
    }

    externalNativeBuild {
        cmake {
            path = file("src/main/cpp/CMakeLists.txt")
            version = "4.1.2"
        }
    }
}

dependencyLocking { lockAllConfigurations() }

dependencies {
    implementation("org.lsposed.libcxx:libcxx-full:30.0.16248370")
    implementation("org.lsposed.hiddenapibypass:hiddenapibypass:6.2")
}
