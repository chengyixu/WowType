// swift-tools-version:6.2
// WowType 的 macOS 前端。不在 Cargo workspace 里；Rust 核心由 scripts/build.sh 先编好。
import PackageDescription

let package = Package(
    name: "WowType",
    platforms: [.macOS(.v13)],
    products: [
        .executable(name: "WowType", targets: ["WowTypeIME"]),
    ],
    targets: [
        // 把 core/include/wowtype_core.h 交给 Swift（只引用，不复制）。
        // 不在这里写 link：静态库由 build.sh 按完整路径链接。
        .systemLibrary(name: "CWowTypeCore", path: "Sources/CWowTypeCore"),
        // 和 InputMethodKit、核心都无关的部分，swift test 直接测。
        .target(name: "WowTypeKit", path: "Sources/WowTypeKit"),
        // 会话和引擎的 Swift 封装；只在主线程上用。
        .target(
            name: "WowTypeEngine",
            dependencies: ["CWowTypeCore", "WowTypeKit"],
            path: "Sources/WowTypeEngine"
        ),
        // 应用里抛出的 NSException 不能让输入法崩掉。
        .target(name: "CObjCSafe", path: "Sources/CObjCSafe"),
        .executableTarget(
            name: "WowTypeIME",
            dependencies: ["WowTypeKit", "WowTypeEngine", "CObjCSafe"],
            path: "Sources/WowTypeIME",
            swiftSettings: [.defaultIsolation(MainActor.self)],
            linkerSettings: [
                .linkedFramework("InputMethodKit"),
                .linkedFramework("Carbon"),
            ]
        ),
        .testTarget(
            name: "WowTypeKitTests",
            dependencies: ["WowTypeKit"],
            path: "Tests/WowTypeKitTests"
        ),
    ]
)
