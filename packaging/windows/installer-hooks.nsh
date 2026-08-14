!macro NSIS_HOOK_POSTINSTALL
  nsExec::ExecToLog '"$INSTDIR\helper\iran-split-helper.exe" --install --mihomo "$INSTDIR\dependencies\mihomo.exe" --staging-dir "$COMMONPROGRAMDATA\iran-split\staging" --tun-name clash-iran'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::ExecToLog '"$INSTDIR\helper\iran-split-helper.exe" --uninstall'
!macroend
