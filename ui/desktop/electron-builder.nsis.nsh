# Per-user only: force the current-user mode so the wizard never offers a per-machine install.
!macro customInstallMode
  StrCpy $isForceCurrentInstall "1"
!macroend
