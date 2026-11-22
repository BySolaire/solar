/** @type {import('@expo/fingerprint').Config} */
module.exports = {
  // Native folders and compiled libraries are regenerated on EAS. Hash their
  // source inputs so local OTA publishing matches a clean cloud build.
  ignorePaths: [
    "ios",
    "ios/**/*",
    "android",
    "android/**/*",
    "modules/solar-link/ios/*.xcframework",
    "modules/solar-link/ios/*.xcframework/**/*",
    "modules/solar-link/android/src/main/jniLibs",
    "modules/solar-link/android/src/main/jniLibs/**/*",
  ],
  extraSources: [
    { type: "file", filePath: "../Cargo.toml", reasons: ["solarRust"] },
    { type: "file", filePath: "../Cargo.lock", reasons: ["solarRust"] },
    {
      type: "dir",
      filePath: "../crates/mobile-link",
      reasons: ["solarRust"],
    },
    {
      type: "file",
      filePath: "scripts/build-native-link.mjs",
      reasons: ["solarRust"],
    },
    {
      type: "file",
      filePath: "scripts/eas-build-pre-install.mjs",
      reasons: ["solarRust"],
    },
  ],
};
