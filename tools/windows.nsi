Unicode true
Name "PURR"
OutFile "${OUTFILE}"
InstallDir "$LOCALAPPDATA\Purr"
RequestExecutionLevel user
SetCompressor /SOLID lzma
!include "MUI2.nsh"
!define MUI_ABORTWARNING
!define MUI_ICON "${APPDIR}/../purr.ico"
!define MUI_UNICON "${APPDIR}/../purr.ico"
!define MUI_FINISHPAGE_RUN "$INSTDIR\Purr.exe"
!define MUI_FINISHPAGE_RUN_TEXT "立即启动 PURR (Launch PURR)"
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${APPDIR}/LICENSE"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "SimpChinese"
!macro KillRunning FunctionName
Function ${FunctionName}
  nsExec::ExecToLog 'taskkill /F /IM Purr.exe /T'
  nsExec::ExecToLog 'taskkill /F /IM mihomo-windows-amd64.exe /T'
  nsExec::ExecToLog 'taskkill /F /IM mihomo.exe /T'
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Internet Settings" "ProxyEnable" 0
  Sleep 400
FunctionEnd
!macroend
!insertmacro KillRunning .onInit
!insertmacro KillRunning un.onInit
Section "PURR" SEC01
  SetShellVarContext current
  nsExec::ExecToLog 'taskkill /F /IM Purr.exe /T'
  nsExec::ExecToLog 'taskkill /F /IM mihomo-windows-amd64.exe /T'
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Internet Settings" "ProxyEnable" 0
  Sleep 200
  SetOutPath "$INSTDIR"
  File /r "${APPDIR}/*"
  File "${APPDIR}/../purr.ico"
  CreateShortcut "$SMPROGRAMS\Purr.lnk" "$INSTDIR\Purr.exe" "" "$INSTDIR\purr.ico" 0
  CreateShortcut "$DESKTOP\Purr.lnk" "$INSTDIR\Purr.exe" "" "$INSTDIR\purr.ico" 0
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Purr" "DisplayName" "PURR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Purr" "DisplayIcon" "$INSTDIR\purr.ico,0"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Purr" "Publisher" "PURR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Purr" "DisplayVersion" "0.1.0"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Purr" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Purr" "InstallLocation" "$INSTDIR"
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Purr" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Purr" "NoRepair" 1
  System::Call 'shell32.dll::SHChangeNotify(i 0x08000000, i 0, i 0, i 0)'
SectionEnd
Section "Uninstall"
  SetShellVarContext current
  nsExec::ExecToLog 'taskkill /F /IM Purr.exe /T'
  nsExec::ExecToLog 'taskkill /F /IM mihomo-windows-amd64.exe /T'
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Internet Settings" "ProxyEnable" 0
  Sleep 200
  Delete "$SMPROGRAMS\Purr.lnk"
  Delete "$DESKTOP\Purr.lnk"
  Delete "$INSTDIR\Purr.exe"
  Delete "$INSTDIR\purr.ico"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\SOFTBUFFER-LICENSE.txt"
  Delete "$INSTDIR\README.txt"
  Delete "$INSTDIR\SOURCES.txt"
  Delete "$INSTDIR\VERIFICATION.md"
  Delete "$INSTDIR\resources\mihomo-windows-amd64.exe"
  Delete "$INSTDIR\resources\wintun.dll"
  Delete "$INSTDIR\resources\GeoIP.dat"
  Delete "$INSTDIR\resources\GeoSite.dat"
  RMDir "$INSTDIR\resources"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Purr"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Purr"
SectionEnd
