package expo.modules.solarlink

import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition

class SolarLinkModule : Module() {
  companion object { init { System.loadLibrary("solar_mobile_link") } }
  private external fun nativeCall(input: String): String
  override fun definition() = ModuleDefinition {
    Name("SolarLink")
    AsyncFunction("execute") { input: String -> nativeCall(input) }
  }
}
