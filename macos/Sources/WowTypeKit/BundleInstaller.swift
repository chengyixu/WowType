// 把 WowType.app 复制到 ~/Library/Input Methods。
//
// 不先删旧的：先复制到同一目录下的临时名字，复制完整了再一次换过去（replaceItemAt），
// 中途失败旧版本原样留着、临时文件清掉。参考 fcitx5-macos 删了一半装不上的教训。
// 注册输入源、关掉旧进程由输入法程序自己做（不在这里）。
import Foundation

public enum BundleInstaller {
    public static let bundleName = "WowType.app"

    public enum Failure: Error, Equatable {
        /// 要装的就是已经装好的那一份。
        case sameLocation
    }

    /// 装到 directory 里，返回装好的位置。
    @discardableResult
    public static func install(source: URL, into directory: URL) throws -> URL {
        let fm = FileManager.default
        let dest = directory.appendingPathComponent(bundleName, isDirectory: true)
        if resolved(source) == resolved(dest) { throw Failure.sameLocation }
        try fm.createDirectory(at: directory, withIntermediateDirectories: true)

        let staging = directory.appendingPathComponent(".\(bundleName).staging-\(UUID().uuidString)", isDirectory: true)
        do {
            try fm.copyItem(at: source, to: staging)
            if fm.fileExists(atPath: dest.path) {
                _ = try fm.replaceItemAt(dest, withItemAt: staging)
            } else {
                try fm.moveItem(at: staging, to: dest)
            }
        } catch {
            try? fm.removeItem(at: staging)
            throw error
        }
        return dest
    }

    /// 删掉 directory 里的 WowType.app。返回 false 表示本来就没有。用户数据不动。
    @discardableResult
    public static func uninstall(from directory: URL) throws -> Bool {
        let dest = directory.appendingPathComponent(bundleName, isDirectory: true)
        guard FileManager.default.fileExists(atPath: dest.path) else { return false }
        try FileManager.default.removeItem(at: dest)
        return true
    }

    /// url 是不是就在 directory 里（解析 .. 和符号链接后比较）。
    public static func isInside(_ url: URL, directory: URL) -> Bool {
        resolved(url.deletingLastPathComponent()) == resolved(directory)
    }

    private static func resolved(_ url: URL) -> String {
        url.standardizedFileURL.resolvingSymlinksInPath().path
    }
}
