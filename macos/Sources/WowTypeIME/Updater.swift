// 在线更新（Sparkle 2）。只在 Info.plist 有可用的更新源和 EdDSA 公钥时启动（WowTypeKit 的
// UpdateConfig）；上游默认构建没有更新源，这里什么都不做。
//
// - 每天在后台查一次（SUEnableAutomaticChecks、SUScheduledCheckInterval，见 build.sh）；
// - 更新包要通过 EdDSA 签名检查，还要和正在运行的版本是同一个开发者签名，Sparkle 才装；
// - 输入法是后台程序：定时查到新版本时用「温和提醒」，不抢正在打字的应用的焦点，
//   菜单里那一项变成「安装新版本 x…」；用户点了才把窗口放到前面；
// - 装好后 Sparkle 重启输入法进程（~/Library/Input Methods 归当前用户，不要管理员密码）。
import AppKit
import Sparkle
import WowTypeKit

@MainActor
final class Updater: NSObject {
    static private(set) var shared: Updater?

    /// 定时检查查到、还没装的新版本（菜单显示用）。
    private(set) var pendingVersion: String?
    private var controller: SPUStandardUpdaterController?

    /// 有可用的更新源才建；在输入法服务启动时调一次。
    static func startIfConfigured(bundle: Bundle = .main) {
        guard shared == nil else { return }
        guard UpdateConfig.from(info: bundle.infoDictionary ?? [:]) != nil else {
            Log.app.info("这一版没有配置在线更新")
            return
        }
        let updater = Updater()
        updater.controller = SPUStandardUpdaterController(
            startingUpdater: true, updaterDelegate: updater, userDriverDelegate: updater)
        shared = updater
    }

    /// 菜单「检查更新…」/「安装新版本…」：用户要的，可以把窗口放到前面。
    func checkForUpdates() {
        NSApp.activate(ignoringOtherApps: true)
        controller?.checkForUpdates(nil)
    }

    var canCheckForUpdates: Bool { controller?.updater.canCheckForUpdates ?? false }
}

extension Updater: SPUUpdaterDelegate {
    func updater(_ updater: SPUUpdater, didAbortWithError error: any Error) {
        Log.app.notice("更新检查没完成: \(error.localizedDescription, privacy: .public)")
    }
}

extension Updater: @preconcurrency SPUStandardUserDriverDelegate {
    var supportsGentleScheduledUpdateReminders: Bool { true }

    /// 定时检查查到新版本：只在 Sparkle 认为现在可以立刻显示（比如刚启动）时让它显示，
    /// 别的时候不弹窗，只在菜单里提示。
    func standardUserDriverShouldHandleShowingScheduledUpdate(
        _ update: SUAppcastItem, andInImmediateFocus immediateFocus: Bool
    ) -> Bool {
        immediateFocus
    }

    func standardUserDriverWillHandleShowingUpdate(
        _ handleShowingUpdate: Bool, forUpdate update: SUAppcastItem, state: SPUUserUpdateState
    ) {
        if !state.userInitiated {
            pendingVersion = update.displayVersionString
        }
    }

    func standardUserDriverDidReceiveUserAttention(forUpdate update: SUAppcastItem) {
        pendingVersion = nil
    }

    func standardUserDriverWillFinishUpdateSession() {
        pendingVersion = nil
    }
}
