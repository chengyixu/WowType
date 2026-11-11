// 在线更新的设置：Info.plist 里有没有可用的 Sparkle 更新源。
//
// 更新源和公钥属于发布方，仓库里不写：build.sh 只在给了 WOWTYPE_UPDATE_FEED_URL 和
// WOWTYPE_UPDATE_PUBLIC_KEY 时写进 Info.plist（SUFeedURL、SUPublicEDKey）。两样都有、都对，
// 输入法才启动更新器、菜单里才有「检查更新…」；缺一样或写错，就当这一版没有在线更新
// （宁可不更新，也不去连一个没法验证签名的源）。
import Foundation

public struct UpdateConfig: Sendable, Equatable {
    public static let feedKey = "SUFeedURL"
    public static let publicKeyKey = "SUPublicEDKey"

    public let feed: URL
    /// EdDSA（ed25519）公钥，base64，解出来正好 32 字节。
    public let publicKey: String

    /// 从 Info.plist 读；没配或配错返回 nil。
    public static func from(info: [String: Any]) -> UpdateConfig? {
        guard let feedText = info[feedKey] as? String,
              let key = info[publicKeyKey] as? String,
              let feed = URL(string: feedText),
              isValid(feed: feed), isValid(publicKey: key) else { return nil }
        return UpdateConfig(feed: feed, publicKey: key)
    }

    /// 只认 https，要有主机名。
    public static func isValid(feed: URL) -> Bool {
        feed.scheme?.lowercased() == "https" && !(feed.host ?? "").isEmpty
    }

    public static func isValid(publicKey: String) -> Bool {
        Data(base64Encoded: publicKey)?.count == 32
    }
}
