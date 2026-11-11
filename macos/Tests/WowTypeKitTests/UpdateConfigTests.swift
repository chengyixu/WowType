import Foundation
import Testing
@testable import WowTypeKit

@Suite("在线更新的设置")
struct UpdateConfigTests {
    private let key = Data(repeating: 7, count: 32).base64EncodedString()
    private let feed = "https://example.com/wowtype/appcast.xml"

    @Test func bothPresentAndValid() {
        let config = UpdateConfig.from(info: [UpdateConfig.feedKey: feed, UpdateConfig.publicKeyKey: key])
        #expect(config == UpdateConfig(feed: URL(string: feed)!, publicKey: key))
    }

    @Test func missingEitherMeansNoUpdates() {
        #expect(UpdateConfig.from(info: [:]) == nil, "上游默认构建：没有更新源")
        #expect(UpdateConfig.from(info: [UpdateConfig.feedKey: feed]) == nil, "没有公钥")
        #expect(UpdateConfig.from(info: [UpdateConfig.publicKeyKey: key]) == nil, "没有更新源")
    }

    @Test func rejectsInsecureOrMalformedFeeds() {
        for bad in ["http://example.com/appcast.xml", "file:///tmp/appcast.xml", "https:///appcast.xml", "", "appcast.xml"] {
            #expect(UpdateConfig.from(info: [UpdateConfig.feedKey: bad, UpdateConfig.publicKeyKey: key]) == nil, "\(bad)")
        }
    }

    @Test func rejectsKeysThatAreNotEd25519() {
        let short = Data(repeating: 7, count: 31).base64EncodedString()
        for bad in [short, "not base64!", "", Data(repeating: 1, count: 64).base64EncodedString()] {
            #expect(UpdateConfig.from(info: [UpdateConfig.feedKey: feed, UpdateConfig.publicKeyKey: bad]) == nil, "\(bad)")
        }
    }
}
