Unicode true
RequestExecutionLevel user
!include "MUI2.nsh"
!include "x64.nsh"

!define PRODUCT_NAME "Direct Payments Timesheets"
!define PUBLISHER "Mark Worsdall"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\DirectPaymentTimesheets"

Name "${PRODUCT_NAME}"
OutFile "${OUTPUT_FILE}"
InstallDir "$LOCALAPPDATA\Programs\DirectPaymentTimesheets"
SetCompressor /SOLID lzma
SetOverwrite on
AllowSkipFiles off
ShowInstDetails show
ShowUninstDetails show
BrandingText "${PRODUCT_NAME}"
!define MUI_ICON "${PROJECT_ROOT}\assets\direct-payment-timesheets.ico"
!define MUI_UNICON "${PROJECT_ROOT}\assets\direct-payment-timesheets.ico"
VIProductVersion "${VERSION}.0"
VIAddVersionKey /LANG=1033 "ProductName" "${PRODUCT_NAME}"
VIAddVersionKey /LANG=1033 "CompanyName" "${PUBLISHER}"
VIAddVersionKey /LANG=1033 "FileDescription" "${PRODUCT_NAME} per-user installer"
VIAddVersionKey /LANG=1033 "FileVersion" "${VERSION}"
VIAddVersionKey /LANG=1033 "ProductVersion" "${VERSION}"

!define MUI_ABORTWARNING
!insertmacro MUI_PAGE_LICENSE "${PROJECT_ROOT}\LICENSE"
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Function .onInit
    SetShellVarContext current
    ${IfNot} ${RunningX64}
        MessageBox MB_OK|MB_ICONSTOP "This installer requires 64-bit Windows."
        Abort
    ${EndIf}
    SetRegView 64
FunctionEnd

Section "Application"
    # Fixed destination, including upgrades and invocations with /D=.
    StrCpy $INSTDIR "$LOCALAPPDATA\Programs\DirectPaymentTimesheets"
    SetShellVarContext current
    SetRegView 64
    SetOutPath "$INSTDIR"
    File /oname=DirectPaymentTimesheets.exe "${PROJECT_ROOT}\target\release\direct_payment_timesheets.exe"
    File "${PROJECT_ROOT}\assets\direct-payment-timesheets.ico"
    File "${PROJECT_ROOT}\LICENSE"
    File "${PROJECT_ROOT}\THIRD_PARTY_LICENSES.md"
    File "${PROJECT_ROOT}\assets\fonts\DejaVu-LICENSE.txt"
    !include "${BUILD_DIR}\third-party-install.nsh"
    SetOutPath "$INSTDIR"
    WriteUninstaller "$INSTDIR\Uninstall.exe"
    CreateDirectory "$SMPROGRAMS\DirectPaymentTimesheets"
    CreateShortcut "$SMPROGRAMS\DirectPaymentTimesheets\Direct Payments Timesheets.lnk" "$INSTDIR\DirectPaymentTimesheets.exe" "" "$INSTDIR\direct-payment-timesheets.ico"
    CreateShortcut "$SMPROGRAMS\DirectPaymentTimesheets\Uninstall.lnk" "$INSTDIR\Uninstall.exe"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "${PRODUCT_NAME}"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "${PUBLISHER}"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" '$"$INSTDIR\DirectPaymentTimesheets.exe$",0'
    WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '$"$INSTDIR\Uninstall.exe$"'
    WriteRegStr HKCU "${UNINSTALL_KEY}" "QuietUninstallString" '$"$INSTDIR\Uninstall.exe$" /S'
    WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
    WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
SectionEnd

Function un.onInit
    SetShellVarContext current
    SetRegView 64
    StrCmp $INSTDIR "$LOCALAPPDATA\Programs\DirectPaymentTimesheets" correct_location
    MessageBox MB_OK|MB_ICONSTOP "Run the uninstaller from the original per-user program installation."
    Abort
correct_location:
FunctionEnd

Section "Uninstall"
    # Never traverse the internal-data or Documents directories. No recursive removal.
    ClearErrors
    Delete "$INSTDIR\DirectPaymentTimesheets.exe"
    IfErrors 0 executable_removed
    MessageBox MB_OK|MB_ICONSTOP "Close Direct Payments Timesheets and run the uninstaller again."
    Abort
executable_removed:
    Delete "$INSTDIR\direct-payment-timesheets.ico"
    Delete "$INSTDIR\LICENSE"
    Delete "$INSTDIR\THIRD_PARTY_LICENSES.md"
    Delete "$INSTDIR\DejaVu-LICENSE.txt"
    !include "${BUILD_DIR}\third-party-uninstall.nsh"
    Delete "$SMPROGRAMS\DirectPaymentTimesheets\Direct Payments Timesheets.lnk"
    Delete "$SMPROGRAMS\DirectPaymentTimesheets\Uninstall.lnk"
    RMDir "$SMPROGRAMS\DirectPaymentTimesheets"
    DeleteRegKey HKCU "${UNINSTALL_KEY}"
    Delete "$INSTDIR\Uninstall.exe"
    RMDir "$INSTDIR"
SectionEnd
