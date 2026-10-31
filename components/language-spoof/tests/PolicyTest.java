// SPDX-License-Identifier: GPL-3.0-only
package dev.duck.toolbox.language;

import android.content.res.Configuration;
import android.os.LocaleList;
import java.lang.reflect.Constructor;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.Locale;
import java.util.Objects;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.atomic.AtomicReference;

public final class PolicyTest {
    private static LanguageHooks policy(String tag) throws Exception {
        Constructor<LanguageHooks> constructor = LanguageHooks.class.getDeclaredConstructor(String.class);
        constructor.setAccessible(true);
        return constructor.newInstance(tag);
    }
    private static LanguageHooks.Hook callback(LanguageHooks policy, LanguageHooks.Kind kind, Method original) throws Exception {
        LanguageHooks.Hook hook = new LanguageHooks.Hook(policy, kind, true);
        if (original != null) {
            Field field = LanguageHooks.Hook.class.getDeclaredField("original");
            field.setAccessible(true); field.set(hook, original);
        }
        return hook;
    }
    public static String property(String key, String fallback) { return fallback; }
    public static Configuration configuration() { return new Configuration(); }
    public static Configuration update(Configuration value) { return value; }
    public static Object failure() { throw new IllegalStateException("original failure"); }
    private static Locale selected;
    public static void setLocale(Locale locale) { selected = Objects.requireNonNull(locale); }
    public static void setLocales(LocaleList locales, int index) {
        Objects.requireNonNull(locales);
        if (locales.isEmpty()) throw new IllegalArgumentException("empty locales");
        if (index < 0 || index >= locales.size()) throw new IndexOutOfBoundsException("index");
        selected = locales.get(index);
    }
    private static void rejected(LanguageHooks.Hook hook, Object[] args, Class<? extends Throwable> type) throws Throwable {
        try { hook.invoke(args); throw new AssertionError("expected " + type.getName()); }
        catch (Throwable error) { if (!type.isInstance(error)) throw error; }
    }
    public static void main(String[] args) throws Throwable {
        for (String tag : new String[]{"en-US", "ja-JP", "zh-Hant-TW", "ar"}) {
            LanguageHooks policy = policy(tag);
            Locale expected = Locale.forLanguageTag(tag);
            Object locale = callback(policy, LanguageHooks.Kind.LOCALE, null).invoke(new Object[0]);
            assert locale.equals(expected);
            LocaleList locales = (LocaleList) callback(policy, LanguageHooks.Kind.LOCALES, null).invoke(new Object[0]);
            assert locales.get(0).equals(expected);
            Method property = PolicyTest.class.getMethod("property", String.class, String.class);
            LanguageHooks.Hook android = callback(policy, LanguageHooks.Kind.PROPERTY, property);
            assert android.invoke(new Object[]{"persist.sys.locale", "real"}).equals(tag);
            assert android.invoke(new Object[]{"persist.sys.language", "real"}).equals(expected.getLanguage());
            assert android.invoke(new Object[]{"ro.product.locale.region", "real"}).equals(expected.getCountry());
            assert android.invoke(new Object[]{"unrelated.property", "original"}).equals("original");
            LanguageHooks.Hook java = callback(policy, LanguageHooks.Kind.JAVA_PROPERTY, property);
            assert java.invoke(new Object[]{"user.language", "real"}).equals(expected.getLanguage());
            assert java.invoke(new Object[]{"user.country", "real"}).equals(expected.getCountry());
            Configuration original = new Configuration();
            Configuration changed = (Configuration) callback(policy, LanguageHooks.Kind.UPDATE,
                PolicyTest.class.getMethod("update", Configuration.class)).invoke(new Object[]{original});
            assert changed != original;
            assert changed.getLocales().get(0).equals(expected);
            assert !original.getLocales().get(0).equals(expected);
            assert changed.densityDpi == original.densityDpi;
            Configuration read = (Configuration) callback(policy, LanguageHooks.Kind.CONFIGURATION,
                PolicyTest.class.getMethod("configuration")).invoke(new Object[0]);
            assert read.getLocales().get(0).equals(expected);
        }
        try {
            callback(policy("en"), LanguageHooks.Kind.CONFIGURATION,
                PolicyTest.class.getMethod("failure")).invoke(new Object[0]);
            throw new AssertionError("expected original exception");
        } catch (IllegalStateException original) { assert original.getMessage().equals("original failure"); }
        LanguageHooks english = policy("en-US");
        rejected(callback(english, LanguageHooks.Kind.LOCALE, null), new Object[]{null}, NullPointerException.class);
        LanguageHooks.Hook setter = callback(english, LanguageHooks.Kind.SET_LOCALE,
            PolicyTest.class.getMethod("setLocale", Locale.class));
        rejected(setter, new Object[]{null}, NullPointerException.class);
        setter.invoke(new Object[]{Locale.JAPAN});
        assert selected.equals(Locale.US);
        LanguageHooks.Hook listSetter = callback(english, LanguageHooks.Kind.SET_LOCALES,
            PolicyTest.class.getMethod("setLocales", LocaleList.class, int.class));
        rejected(listSetter, new Object[]{null, 0}, NullPointerException.class);
        rejected(listSetter, new Object[]{new LocaleList(), 0}, IllegalArgumentException.class);
        rejected(listSetter, new Object[]{new LocaleList(Locale.JAPAN), -1}, IndexOutOfBoundsException.class);
        rejected(listSetter, new Object[]{new LocaleList(Locale.JAPAN), 1}, IndexOutOfBoundsException.class);
        listSetter.invoke(new Object[]{new LocaleList(Locale.JAPAN, Locale.FRANCE), 1});
        assert selected.equals(Locale.US);
        LanguageHooks.Hook pending = callback(english, LanguageHooks.Kind.UPDATE, null);
        AtomicReference<Throwable> failure = new AtomicReference<>();
        CountDownLatch entered = new CountDownLatch(1);
        Thread reader = new Thread(() -> {
            entered.countDown();
            try { pending.invoke(new Object[]{new Configuration()}); }
            catch (Throwable error) { failure.set(error); }
        });
        synchronized (pending) {
            reader.start(); entered.await();
            Field original = LanguageHooks.Hook.class.getDeclaredField("original");
            original.setAccessible(true);
            original.set(pending, PolicyTest.class.getMethod("update", Configuration.class));
        }
        reader.join(3000);
        assert !reader.isAlive() && failure.get() == null;
    }
}
