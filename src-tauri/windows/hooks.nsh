; RigSweep installer hooks.
; Unsigned open-source tools that manage services are often removed by Microsoft Defender's
; machine-learning heuristics (false positive "Trojan:Win32/...!ml"). We never add an
; exclusion silently: the user is asked, and the uninstaller removes it again.

!macro NSIS_HOOK_POSTINSTALL
  ; Silent installs (/S) never get an exclusion.
  IfSilent rigsweep_av_done
  MessageBox MB_YESNO|MB_ICONQUESTION "Allow RigSweep in Microsoft Defender?$\r$\n$\r$\nRigSweep is free, open-source (MIT) and not code-signed yet, so Defender's heuristics sometimes delete it by mistake.$\r$\n$\r$\nYes = exclude only this folder from scanning:$\r$\n$INSTDIR$\r$\n$\r$\nYou can undo this any time in RigSweep > Settings, and the uninstaller removes it." IDNO rigsweep_av_done
    nsExec::ExecToLog 'powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "Add-MpPreference -ExclusionPath $\'$INSTDIR$\'"'
    Pop $0
    StrCmp $0 "0" rigsweep_av_done
      MessageBox MB_OK|MB_ICONINFORMATION "Couldn't add the Defender exclusion (another antivirus may be active, or Tamper Protection blocked it). You can allow RigSweep manually; see the README section 'Antivirus false positives'."
  rigsweep_av_done:
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; Harmless if the exclusion was never added.
  nsExec::ExecToLog 'powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "Remove-MpPreference -ExclusionPath $\'$INSTDIR$\' -ErrorAction SilentlyContinue"'
  Pop $0
!macroend
