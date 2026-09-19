"""Package the already-built and verified Windows pair; does not rebuild."""
import json,shutil,hashlib,subprocess,zipfile,datetime
from pathlib import Path
work=Path(__file__).resolve().parents[1];root=work.parent.parent
old=root/'builds/Pet2-V21-Voice-2026-09-19';out=root/'builds/Pet2-V22-Contact-2026-09-19'
out.mkdir(parents=True,exist_ok=True)
for folder in ['config','licenses','assets']:shutil.copytree(old/folder,out/folder,dirs_exist_ok=True)
for src,dst in [('pet2.exe','Pet2.exe'),('body_lab.exe','Pet2 Dev Console.exe')]:
 source=work/'target/x86_64-pc-windows-msvc/release'/src
 if not (out/dst).exists() or hashlib.sha256(source.read_bytes()).digest()!=hashlib.sha256((out/dst).read_bytes()).digest():shutil.copy2(source,out/dst)
shutil.copy2(work/'docs/V22-contact-hearing.md',out/'V22-notes.md')
(out/'ПРОЧИТАЙ.md').write_text("""# Pet2 V22

Запусти Pet2.exe. Правый клик по питомцу открывает заботу и обучение.

Исправлены поедание крошек с панели, отдельное движение рта, плавность лица и начало расплющивания при контакте. Крошки ярче и вылетают по-разному. Подготовка к рывку, короткое осматривание и отклик на имя зависят от текущего состояния; траектория отклика не записана заранее.

Имя и команды доступны после пяти принятых примеров. Старые 20 примеров имени сохранены и автоматически калибруются. Посторонние слова необязательны. Запись продолжается партиями по пять до 40; Стоп сохраняет ранее завершённые партии. Это обучение звучанию, не свободное распознавание речи.

На главной странице меню можно выбрать отдельный микрофон. При использовании микрофона Bluetooth-гарнитуры Windows может снижать качество воспроизведения. В приложении выход остаётся stereo 48 кГц; отдельный микрофон позволяет избежать разговорного Bluetooth-режима.

Состояние живёт в %LOCALAPPDATA%/lomatoq/Pet 2/data. Перед обновлением сохранена резервная копия backups/pre-v22-2026-09-19. Предыдущая V21 оставлена для восстановления; не запускай два экземпляра одновременно.

Подробности и результаты проверок: V22-notes.md и verification/.
""",encoding='utf-8')
verification=out/'verification';verification.mkdir(exist_ok=True)
for name in ['v22-hearing-tests.log','v22-body-tests.log','v22-app-tests.log','v22-final-clippy.log','v22-saved-name-audit.log','v22-mouth-test.log','v22-face-test.log','v22-field-tests.log']:
 if (work/'target'/name).exists():shutil.copy2(work/'target'/name,verification/name)
for folder in ['v22-ground-food-final','v22-actions-final','v22-menu-final']:
 p=work/'target'/folder/'verification.json'
 if p.exists():shutil.copy2(p,verification/(folder+'.json'))
files={name:{'sha256':hashlib.sha256((out/name).read_bytes()).hexdigest(),'size':(out/name).stat().st_size} for name in ['Pet2.exe','Pet2 Dev Console.exe']}
commit=subprocess.check_output(['git','-c','safe.directory='+str(work).replace(chr(92),'/'),'rev-parse','HEAD'],cwd=work,text=True).strip()
manifest=dict(schema='pet2.release_manifest.v1',release_version='0.1.0',lab_control_protocol=2,lab_control_schema=4,built_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),compatible_pair=True,files=files,source_commit=commit,release_label='Pet2 V22 Contact and Responsive Hearing')
(out/'release-manifest.json').write_text(json.dumps(manifest,indent=2))
archive=out.with_suffix('.zip')
with zipfile.ZipFile(archive,'w',zipfile.ZIP_DEFLATED,compresslevel=6) as z:
 for p in out.rglob('*'):
  if p.is_file():z.write(p,Path(out.name)/p.relative_to(out))
print(json.dumps({'package':str(out),'zip':str(archive),'files':files,'commit':commit},ensure_ascii=False))
