!macro NSIS_HOOK_POSTINSTALL
  nsExec::ExecToLog '"$INSTDIR\helper\iran-split-helper.exe" --install --mihomo "$INSTDIR\dependencies\mihomo.exe" --staging-dir "$LOCALAPPDATA\biflow\runtime\generations" --tun-name clash-iran'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::ExecToLog '"$INSTDIR\helper\iran-split-helper.exe" --uninstall'
!macroend
