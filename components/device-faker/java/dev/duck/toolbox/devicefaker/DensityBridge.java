// SPDX-License-Identifier: GPL-3.0-or-later
package dev.duck.toolbox.devicefaker;

import android.content.ContentResolver;
import android.os.IBinder;
import android.os.Looper;
import org.json.JSONObject;

/** Explicit-user density access; never relies on USER_CURRENT during a mutation. */
public final class DensityBridge {
    private static final String KEY = "display_density_forced";
    private static Object window;
    private static ContentResolver resolver;
    private static Class<?> windowType;

    private static void initialize() throws Exception {
        try {
            Class<?> runtime = Class.forName("dalvik.system.VMRuntime");
            Object instance = runtime.getMethod("getRuntime").invoke(null);
            runtime.getMethod("setHiddenApiExemptions", String[].class)
                .invoke(instance, (Object) new String[]{"L"});
        } catch (ReflectiveOperationException | SecurityException unavailable) {
            // Some app_process environments already permit the required framework calls.
        }
        if (Looper.myLooper() == null) Looper.prepare();
        Class<?> activityThread = Class.forName("android.app.ActivityThread");
        Object thread = activityThread.getMethod("systemMain").invoke(null);
        Object context = activityThread.getMethod("getSystemContext").invoke(thread);
        resolver = (ContentResolver) Class.forName("android.content.Context")
            .getMethod("getContentResolver").invoke(context);
        Object binder = Class.forName("android.os.ServiceManager")
            .getMethod("getService", String.class).invoke(null, "window");
        if (binder == null) throw new IllegalStateException("window service unavailable");
        windowType = Class.forName("android.view.IWindowManager");
        window = Class.forName("android.view.IWindowManager$Stub")
            .getMethod("asInterface", IBinder.class).invoke(null, binder);
    }
    private static int currentUser() throws Exception {
        return (Integer) Class.forName("android.app.ActivityManager")
            .getMethod("getCurrentUser").invoke(null);
    }
    private static String read(int user) throws Exception {
        return (String) Class.forName("android.provider.Settings$Secure")
            .getMethod("getStringForUser", ContentResolver.class, String.class, int.class)
            .invoke(null, resolver, KEY, user);
    }
    private static JSONObject execute(String[] args) throws Exception {
        initialize();
        if (args.length < 1 || args.length > 3) throw new IllegalArgumentException("invalid arguments");
        int user = args.length >= 2 ? Integer.parseInt(args[1]) : currentUser();
        if (user < 0) throw new IllegalArgumentException("explicit user required");
        if (args[0].equals("set") && args.length == 3) {
            if (args[2].equals("reset")) {
                windowType.getMethod("clearForcedDisplayDensityForUser", int.class, int.class)
                    .invoke(window, 0, user);
            } else {
                int density = Integer.parseInt(args[2]);
                if (density < 1 || density > 10000) throw new IllegalArgumentException("invalid density");
                windowType.getMethod("setForcedDisplayDensityForUser", int.class, int.class, int.class)
                    .invoke(window, 0, density, user);
            }
        } else if (!args[0].equals("read") || args.length > 2) {
            throw new IllegalArgumentException("invalid operation");
        }
        String value = read(user);
        // AOSP uses either null or empty for an unset override; both mean reset.
        Integer density = value == null || value.isEmpty() ? null : Integer.valueOf(value);
        return new JSONObject().put("user", user).put("override", density == null ? JSONObject.NULL : density);
    }
    public static void main(String[] args) {
        try {
            System.out.println(new JSONObject().put("ok", true).put("data", execute(args)));
            System.exit(0);
        } catch (Throwable error) {
            // Do not include resolver contents or unrelated framework state in errors.
            System.out.println("{\"ok\":false,\"error\":\"density_bridge_failed\"}");
            System.exit(1);
        }
    }
    private DensityBridge() {}
}
