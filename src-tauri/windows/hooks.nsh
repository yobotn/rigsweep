; RigSweep installer hooks.
; Unsigned open-source tools that manage services are often removed by Microsoft Defender's
; machine-learning heuristics (false positive "Trojan:Win32/...!ml"). We never add an
; exclusion silently: the user is asked, and the uninstaller removes it again.
; The prompt follows the installer language ($LANGUAGE = Windows LANGID). Generated text:
; keep this file UTF-8 with BOM so NSIS reads the translations correctly.

!macro NSIS_HOOK_POSTINSTALL
  ; Silent installs (/S) never get an exclusion.
  IfSilent rigsweep_av_done
  StrCpy $R1 "Allow RigSweep in Microsoft Defender?$\r$\n$\r$\nRigSweep is free, open-source (MIT) and not code-signed yet, so Defender's heuristics sometimes delete it by mistake.$\r$\n$\r$\nYes = exclude only this folder from scanning:$\r$\n$INSTDIR$\r$\n$\r$\nYou can undo this any time in RigSweep > Settings, and the uninstaller removes it."
  StrCmp $LANGUAGE 1031 0 +2
    StrCpy $R1 "RigSweep in Microsoft Defender zulassen?$\r$\n$\r$\nRigSweep ist kostenlos, Open Source (MIT) und noch nicht codesigniert. Die Heuristik von Defender löscht es deshalb manchmal irrtümlich.$\r$\n$\r$\nJa = nur diesen Ordner von der Überprüfung ausschließen:$\r$\n$INSTDIR$\r$\n$\r$\nSie können das jederzeit unter RigSweep > Einstellungen rückgängig machen, und bei der Deinstallation wird der Ausschluss entfernt."
  StrCmp $LANGUAGE 1034 0 +2
    StrCpy $R1 "¿Permitir RigSweep en Microsoft Defender?$\r$\n$\r$\nRigSweep es gratuito, de código abierto (MIT) y todavía no tiene firma de código, por lo que la heurística de Defender a veces lo elimina por error.$\r$\n$\r$\nSí = excluir del examen solo esta carpeta:$\r$\n$INSTDIR$\r$\n$\r$\nPuedes deshacerlo cuando quieras en RigSweep > Configuración, y el desinstalador quita la exclusión."
  StrCmp $LANGUAGE 1036 0 +2
    StrCpy $R1 "Autoriser RigSweep dans Microsoft Defender ?$\r$\n$\r$\nRigSweep est gratuit, open source (MIT) et pas encore signé numériquement : les heuristiques de Defender le suppriment donc parfois par erreur.$\r$\n$\r$\nOui = exclure de l'analyse uniquement ce dossier :$\r$\n$INSTDIR$\r$\n$\r$\nVous pouvez annuler ce choix à tout moment dans RigSweep > Paramètres, et la désinstallation supprime l'exclusion."
  StrCmp $LANGUAGE 1040 0 +2
    StrCpy $R1 "Consentire RigSweep in Microsoft Defender?$\r$\n$\r$\nRigSweep è gratuito, open source (MIT) e non ancora firmato digitalmente, quindi l'euristica di Defender a volte lo elimina per errore.$\r$\n$\r$\nSì = escludi dall'analisi solo questa cartella:$\r$\n$INSTDIR$\r$\n$\r$\nPuoi annullare in qualsiasi momento da RigSweep > Impostazioni, e la disinstallazione rimuove l'esclusione."
  StrCmp $LANGUAGE 1046 0 +2
    StrCpy $R1 "Permitir o RigSweep no Microsoft Defender?$\r$\n$\r$\nO RigSweep é gratuito, de código aberto (MIT) e ainda não tem assinatura de código, por isso a heurística do Defender às vezes o exclui por engano.$\r$\n$\r$\nSim = excluir da verificação apenas esta pasta:$\r$\n$INSTDIR$\r$\n$\r$\nVocê pode desfazer isso a qualquer momento em RigSweep > Configurações, e o desinstalador remove a exclusão."
  StrCmp $LANGUAGE 1049 0 +2
    StrCpy $R1 "Разрешить RigSweep в Microsoft Defender?$\r$\n$\r$\nRigSweep — бесплатная программа с открытым исходным кодом (MIT), пока без цифровой подписи, поэтому эвристика Defender иногда удаляет ее по ошибке.$\r$\n$\r$\nДа — исключить из проверки только эту папку:$\r$\n$INSTDIR$\r$\n$\r$\nОтменить это можно в любой момент в разделе RigSweep > Параметры, а при удалении программы исключение будет снято."
  StrCmp $LANGUAGE 1041 0 +2
    StrCpy $R1 "Microsoft Defender で RigSweep を許可しますか?$\r$\n$\r$\nRigSweep は無料のオープンソース (MIT) ですが、まだコード署名されていないため、Defender のヒューリスティックによって誤って削除されることがあります。$\r$\n$\r$\n[はい] = このフォルダーのみスキャンから除外します:$\r$\n$INSTDIR$\r$\n$\r$\nこの設定は RigSweep の [設定] からいつでも元に戻せます。アンインストール時にも除外は削除されます。"
  StrCmp $LANGUAGE 1042 0 +2
    StrCpy $R1 "Microsoft Defender에서 RigSweep을 허용할까요?$\r$\n$\r$\nRigSweep은 무료 오픈 소스(MIT)이며 아직 코드 서명이 되어 있지 않아 Defender의 휴리스틱이 실수로 삭제하는 경우가 있습니다.$\r$\n$\r$\n예 = 이 폴더만 검사에서 제외:$\r$\n$INSTDIR$\r$\n$\r$\n언제든지 RigSweep > 설정에서 취소할 수 있으며, 제거 프로그램이 제외 항목을 삭제합니다."
  StrCmp $LANGUAGE 2052 0 +2
    StrCpy $R1 "要在 Microsoft Defender 中允许 RigSweep 吗？$\r$\n$\r$\nRigSweep 是免费的开源软件（MIT），目前尚未进行代码签名，因此 Defender 的启发式检测有时会误删它。$\r$\n$\r$\n是 = 仅将此文件夹排除在扫描之外：$\r$\n$INSTDIR$\r$\n$\r$\n你可以随时在 RigSweep > 设置中撤消此操作，卸载时也会移除该排除项。"
  StrCmp $LANGUAGE 1037 0 +2
    StrCpy $R1 "לאפשר את RigSweep ב-Microsoft Defender?$\r$\n$\r$\nRigSweep היא תוכנה חינמית בקוד פתוח (MIT) שעדיין אינה חתומה דיגיטלית, ולכן ההיוריסטיקות של Defender מוחקות אותה לפעמים בטעות.$\r$\n$\r$\nכן = להחריג מהסריקה רק את התיקייה הזו:$\r$\n$INSTDIR$\r$\n$\r$\nאפשר לבטל זאת בכל עת ב-RigSweep ← הגדרות, ותוכנית ההסרה מסירה את אי ההכללה."
  StrCmp $LANGUAGE 1025 0 +2
    StrCpy $R1 "هل تريد السماح بـ RigSweep في Microsoft Defender؟$\r$\n$\r$\nإن RigSweep مجاني ومفتوح المصدر (MIT) لكنه غير موقّع رقميًا بعد، لذا قد تحذفه خوارزميات Defender الاستدلالية أحيانًا عن طريق الخطأ.$\r$\n$\r$\nنعم = استثناء هذا المجلد وحده من الفحص:$\r$\n$INSTDIR$\r$\n$\r$\nيمكنك التراجع عن ذلك في أي وقت من RigSweep ← الإعدادات، كما يزيله برنامج إلغاء التثبيت."
  MessageBox MB_YESNO|MB_ICONQUESTION "$R1" IDNO rigsweep_av_done
    nsExec::ExecToLog 'powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "Add-MpPreference -ExclusionPath $\'$INSTDIR$\'"'
    Pop $0
    StrCmp $0 "0" rigsweep_av_done
  StrCpy $R1 "Couldn't add the Defender exclusion (another antivirus may be active, or Tamper Protection blocked it). You can allow RigSweep manually; see the README section 'Antivirus false positives'."
  StrCmp $LANGUAGE 1031 0 +2
    StrCpy $R1 "Der Defender-Ausschluss konnte nicht hinzugefügt werden (möglicherweise ist ein anderes Antivirenprogramm aktiv, oder der Manipulationsschutz hat ihn blockiert). Sie können RigSweep manuell zulassen; siehe den README-Abschnitt „Antivirus-Fehlalarme“."
  StrCmp $LANGUAGE 1034 0 +2
    StrCpy $R1 "No se pudo agregar la exclusión de Defender (puede haber otro antivirus activo o la Protección contra alteraciones la bloqueó). Puedes permitir RigSweep manualmente; consulta la sección «Falsos positivos del antivirus» del README."
  StrCmp $LANGUAGE 1036 0 +2
    StrCpy $R1 "Impossible d'ajouter l'exclusion Defender (un autre antivirus est peut-être actif, ou la protection contre les falsifications l'a bloquée). Vous pouvez autoriser RigSweep manuellement : voir la section « Faux positifs antivirus » du README."
  StrCmp $LANGUAGE 1040 0 +2
    StrCpy $R1 "Impossibile aggiungere l'esclusione di Defender (potrebbe essere attivo un altro antivirus, oppure la protezione anti-manomissione l'ha bloccata). Puoi consentire RigSweep manualmente: vedi la sezione “Falsi positivi antivirus” del README."
  StrCmp $LANGUAGE 1046 0 +2
    StrCpy $R1 "Não foi possível adicionar a exclusão do Defender (outro antivírus pode estar ativo ou a Proteção contra Adulteração bloqueou a ação). Você pode permitir o RigSweep manualmente; consulte a seção “Falsos positivos do antivírus” do README."
  StrCmp $LANGUAGE 1049 0 +2
    StrCpy $R1 "Не удалось добавить исключение Defender (возможно, работает другой антивирус или его заблокировала защита от подделки). Разрешите RigSweep вручную: см. раздел README «Ложные срабатывания антивируса»."
  StrCmp $LANGUAGE 1041 0 +2
    StrCpy $R1 "Defender の除外を追加できませんでした (別のウイルス対策ソフトが有効になっているか、改ざん防止機能によってブロックされた可能性があります)。手動で RigSweep を許可できます。README の「ウイルス対策ソフトの誤検知」を参照してください。"
  StrCmp $LANGUAGE 1042 0 +2
    StrCpy $R1 "Defender 제외 항목을 추가하지 못했습니다(다른 바이러스 백신이 활성화되어 있거나 변조 방지 기능이 차단했을 수 있음). RigSweep을 직접 허용하세요. README의 '바이러스 백신 오탐지' 섹션을 참조하세요."
  StrCmp $LANGUAGE 2052 0 +2
    StrCpy $R1 "无法添加 Defender 排除项（可能有其他防病毒软件处于活动状态，或篡改防护阻止了此操作）。你可以手动允许 RigSweep；请参阅 README 中的“防病毒软件误报”部分。"
  StrCmp $LANGUAGE 1037 0 +2
    StrCpy $R1 "לא ניתן היה להוסיף את אי ההכללה ל-Defender (ייתכן שאנטי-וירוס אחר פעיל, או שההגנה מפני חבלה חסמה זאת). אפשר לאפשר את RigSweep ידנית; פרטים בסעיף 'התראות שווא של אנטי-וירוס' ב-README."
  StrCmp $LANGUAGE 1025 0 +2
    StrCpy $R1 "تعذّرت إضافة استثناء Defender (ربما يكون برنامج آخر لمكافحة الفيروسات نشطًا، أو منعت ميزة «الحماية من العبث» ذلك). يمكنك السماح بـ RigSweep يدويًا؛ راجع القسم «الإنذارات الكاذبة من برامج مكافحة الفيروسات» في ملف README."
      MessageBox MB_OK|MB_ICONINFORMATION "$R1"
  rigsweep_av_done:
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; Harmless if the exclusion was never added.
  nsExec::ExecToLog 'powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "Remove-MpPreference -ExclusionPath $\'$INSTDIR$\' -ErrorAction SilentlyContinue"'
  Pop $0
!macroend
