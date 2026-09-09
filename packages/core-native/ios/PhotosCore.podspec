require 'json'

package = JSON.parse(File.read(File.join(__dir__, '..', 'package.json')))

Pod::Spec.new do |s|
  s.name           = 'PhotosCore'
  s.version        = package['version']
  s.summary        = package['description']
  s.license        = package['license']
  s.author         = 'Photos contributors'
  s.homepage       = 'https://github.com/horacioh/photos'
  s.platforms      = { :ios => '15.1' }
  s.swift_version  = '5.9'
  s.source         = { git: '' }
  s.static_framework = true

  s.dependency 'ExpoModulesCore'

  # Swift wrapper + UniFFI-generated Swift (ios/generated/*.swift, gitignored).
  s.source_files = '*.swift', 'generated/*.swift'
  # UniFFI C header + modulemap for the Rust static library.
  s.public_header_files = 'generated/*.h'
  s.preserve_paths = 'generated/*.modulemap'
  # XCFramework produced by scripts/build-rust.sh from crates/core-uniffi.
  s.vendored_frameworks = 'generated/PhotosCoreFFI.xcframework'

  s.pod_target_xcconfig = {
    'DEFINES_MODULE' => 'YES',
    'SWIFT_INCLUDE_PATHS' => '$(PODS_TARGET_SRCROOT)/generated'
  }
end
