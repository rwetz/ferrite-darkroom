; Shared Ferrite per-user setup. Build with package_windows.py.
Unicode true
!include "MUI2.nsh"
!include "x64.nsh"
Name "${PRODUCT}"
OutFile "${OUTPUT}"
InstallDir "$LOCALAPPDATA\Programs\Ferrite\${BINARY}"
InstallDirRegKey HKCU "Software\Ferrite\${BINARY}" "InstallDir"
RequestExecutionLevel user
SetCompressor /SOLID lzma
!define MUI_ABORTWARNING
!define MUI_ICON "${ICON}"
!define MUI_UNICON "${ICON}"
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${SOURCE}\LICENSE"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_OK|MB_ICONSTOP "This release requires 64-bit Windows."
    Abort
  ${EndIf}
FunctionEnd

Section "${PRODUCT}"
  SetShellVarContext current
  SetOutPath "$INSTDIR"
  ; Abort instead of silently succeeding when a running app locks the binary.
  SetOverwrite on
  ClearErrors
  File /r "${SOURCE}\*"
  IfErrors 0 +3
    MessageBox MB_OK|MB_ICONSTOP "Close ${PRODUCT}, then run setup again."
    Abort
  WriteUninstaller "$INSTDIR\uninstall.exe"
  CreateDirectory "$SMPROGRAMS\Ferrite"
  CreateShortCut "$SMPROGRAMS\Ferrite\${PRODUCT}.lnk" "$INSTDIR\${BINARY}.exe"
  WriteRegStr HKCU "Software\Ferrite\${BINARY}" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Ferrite-${BINARY}" "DisplayName" "${PRODUCT}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Ferrite-${BINARY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Ferrite-${BINARY}" "Publisher" "Ryan Wetzstein"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Ferrite-${BINARY}" "DisplayIcon" "$INSTDIR\${BINARY}.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Ferrite-${BINARY}" "UninstallString" '$\"$INSTDIR\uninstall.exe$\"'
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Ferrite-${BINARY}" "QuietUninstallString" '$\"$INSTDIR\uninstall.exe$\" /S'
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Ferrite-${BINARY}" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Ferrite-${BINARY}" "NoRepair" 1
SectionEnd

Section "Uninstall"
  SetShellVarContext current
  ; Only delete files owned by this package. Never recurse into user data.
  ClearErrors
  Delete "$INSTDIR\${BINARY}.exe"
  IfErrors 0 +3
    MessageBox MB_OK|MB_ICONSTOP "Close ${PRODUCT}, then uninstall again."
    Abort
  Delete "$INSTDIR\${BINARY}.old"
  Delete "$INSTDIR\README.md"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\NOTICE"
  Delete "$INSTDIR\CHANGELOG.md"
  Delete "$INSTDIR\licenses\LICENSE-*"
  RMDir "$INSTDIR\licenses"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\Ferrite\${PRODUCT}.lnk"
  RMDir "$SMPROGRAMS\Ferrite"
  DeleteRegKey HKCU "Software\Ferrite\${BINARY}"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Ferrite-${BINARY}"
SectionEnd
