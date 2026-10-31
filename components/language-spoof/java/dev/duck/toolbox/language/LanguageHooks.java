// SPDX-License-Identifier: GPL-3.0-only
package dev.duck.toolbox.language;

import android.content.res.Configuration;
import android.content.res.Resources;
import android.os.LocaleList;
import android.os.Build;
import org.lsposed.hiddenapibypass.HiddenApiBypass;
import android.util.Log;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Locale;

/** Immutable process-local policy; never writes settings or system properties. */
public final class LanguageHooks {
    private final Locale locale;
    private final LocaleList locales;
    private final List<Hook> installed = new ArrayList<>();

    private LanguageHooks(String tag) {
        locale = Locale.forLanguageTag(tag);
        locales = new LocaleList(locale);
    }

    private static native Method hook(Method target, Object owner, Method callback);
    private static native boolean deoptimize(Method method);

    public static int install(String tag) {
        if (Build.VERSION.SDK_INT >= 28) {
            HiddenApiBypass.addHiddenApiExemptions("Landroid/content/res/ResourcesImpl;",
                "Landroid/os/SystemProperties;", "Landroid/os/LocaleList;",
                "Landroid/app/IActivityManager$Stub$Proxy;", "Landroid/app/ActivityThread;",
                "Landroid/app/ResourcesManager;", "Landroid/app/ConfigurationController;");
        }
        LanguageHooks policy = new LanguageHooks(tag);
        Locale.setDefault(policy.locale);
        Locale.setDefault(Locale.Category.DISPLAY, policy.locale);
        Locale.setDefault(Locale.Category.FORMAT, policy.locale);
        LocaleList.setDefault(policy.locales);
        policy.systemProperties();
        policy.methods(Locale.class, "getDefault", Kind.LOCALE);
        policy.methods(Locale.class, "setDefault", Kind.SET_LOCALE);
        policy.methods(LocaleList.class, "getDefault", Kind.LOCALES);
        policy.methods(LocaleList.class, "getAdjustedDefault", Kind.LOCALES);
        policy.methods(LocaleList.class, "setDefault", Kind.SET_LOCALES);
        policy.methods(Configuration.class, "getLocales", Kind.LOCALES);
        policy.methods(Resources.class, "getConfiguration", Kind.CONFIGURATION);
        policy.optional("android.content.res.ResourcesImpl", "updateConfiguration", Kind.UPDATE);
        policy.methods(Resources.class, "updateConfiguration", Kind.UPDATE);
        policy.optional("android.app.LocaleManager", "getSystemLocales", Kind.LOCALES);
        policy.optional("android.app.LocaleManager", "getApplicationLocales", Kind.APP_LOCALES);
        policy.optional("android.app.IActivityManager$Stub$Proxy", "getConfiguration", Kind.CONFIGURATION);
        policy.optional("android.os.SystemProperties", "get", Kind.PROPERTY);
        policy.methods(System.class, "getProperty", Kind.JAVA_PROPERTY);
        policy.deoptimize("android.app.ActivityThread", "handleBindApplication",
            "handleConfigurationChanged", "handleActivityConfigurationChanged");
        policy.deoptimize("android.app.ResourcesManager", "applyConfigurationToResources");
        policy.deoptimize("android.app.ConfigurationController", "handleConfigurationChanged");
        Resources system = Resources.getSystem();
        Configuration configuration = new Configuration(system.getConfiguration());
        policy.configure(configuration);
        system.updateConfiguration(configuration, system.getDisplayMetrics());
        return policy.installed.size();
    }

    private void systemProperties() {
        System.setProperty("user.language", locale.getLanguage());
        System.setProperty("user.country", locale.getCountry());
        System.setProperty("user.variant", locale.getVariant());
        System.setProperty("user.script", locale.getScript());
    }

    private void configure(Configuration configuration) {
        configuration.setLocales(locales);
        configuration.setLayoutDirection(locale);
    }

    private void optional(String clazz, String name, Kind kind) {
        try { methods(Class.forName(clazz), name, kind); }
        catch (ClassNotFoundException | LinkageError | RuntimeException ignored) {
            Log.w("DuckLanguage", "Optional language interface unavailable: " + clazz);
        }
    }

    private void deoptimize(String clazz, String... names) {
        try {
            for (Method method : Class.forName(clazz).getDeclaredMethods()) {
                if (Arrays.asList(names).contains(method.getName())) {
                    method.setAccessible(true);
                    if (!deoptimize(method)) Log.w("DuckLanguage", "Framework caller could not be deoptimized");
                }
            }
        } catch (ClassNotFoundException | LinkageError | RuntimeException ignored) {
            Log.w("DuckLanguage", "Framework caller unavailable: " + clazz);
        }
    }

    private void methods(Class<?> clazz, String name, Kind kind) {
        for (Method method : clazz.getDeclaredMethods()) {
            if (!method.getName().equals(name) || Modifier.isAbstract(method.getModifiers())) continue;
            if (kind == Kind.APP_LOCALES && method.getParameterCount() != 0) continue;
            if (kind == Kind.PROPERTY || kind == Kind.JAVA_PROPERTY) {
                if (method.getReturnType() != String.class || method.getParameterCount() < 1
                    || method.getParameterTypes()[0] != String.class) continue;
            }
            if (kind == Kind.UPDATE && !Arrays.asList(method.getParameterTypes()).contains(Configuration.class)) continue;
            Hook callback = new Hook(this, kind, Modifier.isStatic(method.getModifiers()));
            try {
                method.setAccessible(true);
                Method receiver = Hook.class.getDeclaredMethod("invoke", Object[].class);
                synchronized (callback) {
                    Method backup = hook(method, callback, receiver);
                    if (backup != null) {
                        backup.setAccessible(true);
                        callback.original = backup;
                        installed.add(callback);
                    } else Log.w("DuckLanguage", "Unavailable language hook: " + clazz.getName() + "." + name);
                }
            } catch (ReflectiveOperationException | RuntimeException error) {
                Log.w("DuckLanguage", "Language hook rejected: " + clazz.getName() + "." + name);
            }
        }
    }

    enum Kind { LOCALE, LOCALES, SET_LOCALE, SET_LOCALES, CONFIGURATION, UPDATE, PROPERTY, JAVA_PROPERTY, APP_LOCALES }

    public static final class Hook {
        private final LanguageHooks policy;
        private final Kind kind;
        private final boolean isStatic;
        private Method original;

        Hook(LanguageHooks policy, Kind kind, boolean isStatic) {
            this.policy = policy; this.kind = kind; this.isStatic = isStatic;
        }

        public Object invoke(Object[] args) throws Throwable {
            switch (kind) {
                case LOCALE:
                    if (args.length > 0 && args[0] == null) throw new NullPointerException("category");
                    return policy.locale;
                case LOCALES:
                case APP_LOCALES: return policy.locales;
                default: break;
            }
            int offset = isStatic ? 0 : 1;
            Object receiver = isStatic ? null : args[0];
            Object[] parameters = Arrays.copyOfRange(args, offset, args.length);
            if (kind == Kind.SET_LOCALE && Arrays.stream(parameters).allMatch(value -> value != null))
                parameters[parameters.length - 1] = policy.locale;
            if (kind == Kind.SET_LOCALES && parameters[0] instanceof LocaleList
                && !((LocaleList) parameters[0]).isEmpty()
                && (parameters.length == 1 || ((Integer) parameters[1] >= 0
                    && (Integer) parameters[1] < ((LocaleList) parameters[0]).size()))) {
                parameters[0] = policy.locales;
                if (parameters.length == 2) parameters[1] = 0;
            }
            if (kind == Kind.UPDATE) {
                for (int i = 0; i < parameters.length; i++) {
                    if (parameters[i] instanceof Configuration) {
                        Configuration copy = new Configuration((Configuration) parameters[i]);
                        policy.configure(copy);
                        parameters[i] = copy;
                    }
                }
            }
            if (kind == Kind.PROPERTY || kind == Kind.JAVA_PROPERTY) {
                String replacement = property((String) parameters[0], kind == Kind.JAVA_PROPERTY);
                if (replacement != null) return replacement;
            }
            Object result;
            Method backup;
            // Other threads wait until installation publishes an accessible backup.
            // Constant-return getters above do not need this monitor.
            synchronized (this) { backup = original; }
            if (backup == null) throw new IllegalStateException("Language hook backup unavailable");
            try { result = backup.invoke(receiver, parameters); }
            catch (InvocationTargetException failure) { throw failure.getCause(); }
            if (kind == Kind.CONFIGURATION && result instanceof Configuration) {
                Configuration copy = new Configuration((Configuration) result);
                policy.configure(copy);
                return copy;
            }
            return result;
        }

        private String property(String name, boolean java) {
            if (name == null) return null;
            if (java) {
                switch (name) {
                    case "user.language": return policy.locale.getLanguage();
                    case "user.country": return policy.locale.getCountry();
                    case "user.variant": return policy.locale.getVariant();
                    case "user.script": return policy.locale.getScript();
                    default: return null;
                }
            }
            switch (name) {
                case "persist.sys.locale":
                case "ro.product.locale": return policy.locale.toLanguageTag();
                case "persist.sys.language":
                case "ro.product.locale.language": return policy.locale.getLanguage();
                case "persist.sys.country":
                case "ro.product.locale.region": return policy.locale.getCountry();
                default: return null;
            }
        }
    }
}
