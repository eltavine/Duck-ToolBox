package android.app;

import android.app.IUidObserver;

interface IActivityManager {
    // 占位（真实 IActivityManager 第 1 个方法，native 侧稳定区段）；
    // registerUidObserver 必须保持第 2 位（code=2）才能命中真实服务。
    void openContentUri(in String uriString);
    void registerUidObserver(in IUidObserver observer, int which, int cutpoint, String callingPackage);
}
