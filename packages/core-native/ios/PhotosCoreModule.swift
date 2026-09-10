import ExpoModulesCore

// Thin Expo Module over the UniFFI-generated Swift bindings (`ios/generated`,
// produced by `bun run --filter @photos/core-native build:rust`). Keeps the JS
// surface identical to the WASM build: JSON in, JSON out.
public class PhotosCoreModule: Module {
  private let core = Core()

  public func definition() -> ModuleDefinition {
    Name("PhotosCore")

    Function("version") { () -> String in
      version()
    }

    Function("dispatch") { (eventJson: String) throws in
      try core.dispatch(eventJson: eventJson)
    }

    Function("stateJson") { () -> String in
      core.stateJson()
    }
  }
}
