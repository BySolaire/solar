Unicode True
!include "MUI2.nsh"
!include "FileFunc.nsh"
Name "Solar"
OutFile "@OUTFILE@"
InstallDir "$LOCALAPPDATA\Programs\Solar"
InstallDirRegKey HKCU "Software\Solar" "InstallDir"
RequestExecutionLevel user
SetCompressor /SOLID lzma
VIProductVersion "@WINDOWS_VERSION@"
VIAddVersionKey "ProductName" "Solar"
VIAddVersionKey "FileDescription" "Solar preview installer"
VIAddVersionKey "FileVersion" "@VERSION@"
!define MUI_ICON "@ICON@"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"
Var UpdatePID
Var HadPrevious
Function .onInit
  ${GetParameters} $R0
  ${GetOptions} $R0 "/UPDATEPID=" $UpdatePID
  StrCmp $UpdatePID "" ready
  System::Call 'kernel32::OpenProcess(i 0x00100000, i 0, i $UpdatePID) p .r1'
  StrCmp $1 0 ready
  System::Call 'kernel32::WaitForSingleObject(p r1, i 120000) i .r2'
  System::Call 'kernel32::CloseHandle(p r1)'
  StrCmp $2 0 ready
  Abort "Solar did not close. Please try updating again."
  ready:
FunctionEnd
Section "Solar"
  SetOutPath "$INSTDIR"
  ClearErrors
  File /oname=solar.next.exe "@STAGE@\solar.exe"
  IfErrors install_failed
  StrCpy $HadPrevious 0
  IfFileExists "$INSTDIR\solar.exe" 0 replace_binary
  Delete "$INSTDIR\solar.previous.exe"
  ClearErrors
  Rename "$INSTDIR\solar.exe" "$INSTDIR\solar.previous.exe"
  IfErrors install_failed
  StrCpy $HadPrevious 1
  replace_binary:
  ClearErrors
  Rename "$INSTDIR\solar.next.exe" "$INSTDIR\solar.exe"
  IfErrors rollback_binary
  File "@STAGE@\native-install.json"
  File /r "@STAGE@\licenses"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  WriteRegStr HKCU "Software\Solar" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\SolarNative" "DisplayName" "Solar"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\SolarNative" "DisplayVersion" "@VERSION@"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\SolarNative" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  CreateShortcut "$SMPROGRAMS\Solar.lnk" "$INSTDIR\solar.exe"
  CreateShortcut "$DESKTOP\Solar.lnk" "$INSTDIR\solar.exe"
  Delete "$INSTDIR\solar.previous.exe"
  Goto install_done
  rollback_binary:
  StrCmp $HadPrevious 1 0 install_failed
  Rename "$INSTDIR\solar.previous.exe" "$INSTDIR\solar.exe"
  install_failed:
  Delete "$INSTDIR\solar.next.exe"
  MessageBox MB_ICONSTOP "Solar could not be updated. The previous executable was retained. Close other Solar processes and try again."
  Abort
  install_done:
SectionEnd
Function .onInstFailed
  StrCmp $UpdatePID "" failed_done
  IfFileExists "$INSTDIR\solar.exe" 0 failed_done
  Exec '"$INSTDIR\solar.exe"'
  failed_done:
FunctionEnd
Function .onInstSuccess
  StrCmp $UpdatePID "" done
  Exec '"$INSTDIR\solar.exe"'
  done:
FunctionEnd
Section "Uninstall"
  Delete "$SMPROGRAMS\Solar.lnk"
  Delete "$DESKTOP\Solar.lnk"
  Delete "$INSTDIR\solar.exe"
  Delete "$INSTDIR\native-install.json"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir /r "$INSTDIR\licenses"
  RMDir "$INSTDIR"
  DeleteRegKey HKCU "Software\Solar"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\SolarNative"
SectionEnd
