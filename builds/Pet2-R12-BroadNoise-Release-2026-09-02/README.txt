PET 2 — R12 Broad Noise Release — 2026-09-02

Гэта production-зборка з Pet Lab. Pet2.exe — роўна той binary, выгляд якога
быў пацверджаны ў жывым запуску 2026-09-02.

Запуск без лагаў:
- START-PET.cmd — толькі production Pet, без --dev-mode і без Lab.

Наладка:
- START-LAB.cmd — адкрыць Pet Lab побач з ужо запушчаным Pet.
- START-PET-AND-LAB.cmd — запусціць Pet у --dev-mode і агульны Lab з укладкамі
  Body, Den / home, Voice + sounds і Nervous → body. F12 пераключае live telemetry.
  Спачатку закрый ужо запушчаны Pet2.exe, каб не было двух overlay адначасова.

Захаваныя настройкі:
- profile: PET-2 Production Black
- schema: 21
- den noise width: 15
- den displacement radius: 1.15
- broad inward noise без чорнай мяжы
- desktop displacement не знікае ў нулявой фазе inward ripple
- character рэндэрыцца паверх den

На гэтым ПК Pet 2 чытае жывы профіль з:
%LOCALAPPDATA%\lomatoq\Pet 2\data\liquid-tuning.json

Копія зацверджанага профілю таксама знаходзіцца ў:
config\embodiment\active-liquid-profile-r11.json

У binary ушыты той жа production fallback, таму пры адсутнасці AppData-профілю
запуск не верне старыя den-настройкі.
