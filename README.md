# TPrompt

Телепромптър в един HTML файл (`www/index.html`), който се пакетира за:

- **Windows**: приложение с Tauri, само инсталатор (NSIS `.exe`), без преносима версия;
- **Android**: приложение с Capacitor (`.apk`). Работи самостоятелно или като редактор и дистанционно към компютъра.

Двете приложения споделят един и същ `www/index.html`. Сглобяването става в GitHub Actions, не е нужно нищо да се инсталира на компютъра.

## Структура

```
TPrompt/
├── www/index.html              приложението (споделено)
├── src-tauri/                  Windows (Tauri 2, Rust)
│   ├── src/lib.rs              команди: запазване, втори прозорец, сървър за дистанционно, старт с Windows
│   ├── src/remote.html         уеб страница за дистанционно (отваря се от браузър на телефона)
│   ├── tauri.conf.json
│   └── icons/
├── capacitor.config.json       Android (Capacitor 7)
├── scripts/
│   ├── fetch-vendor.sh         сваля jsQR и mammoth в www/vendor
│   └── android-patch.py        разрешения за камера, http връзка, икони
├── resources/icon-source.png   изходна икона 1024×1024
├── package.json
└── .github/workflows/
    ├── build-windows.yml
    └── build-android.yml
```

Папката `android/` не е в хранилището: създава се наново при всяко сглобяване с `npx cap add android`.

## Как се ползва в GitHub

1. Създайте ново хранилище и качете съдържанието на тази папка (самите файлове, не външната папка).
2. Във вкладка **Actions** двата процеса се пускат сами при всяко качване в `main`. Могат да се пуснат и ръчно (**Run workflow**).
3. Готовите файлове са в края на всяко изпълнение, в **Artifacts**:
   - `TPrompt-Windows-installer`: инсталаторът `TPrompt_1.0.0_x64-setup.exe`;
   - `TPrompt-Android-apk`: `TPrompt.apk`.
4. За публикуване във **Releases** създайте таг, например:
   ```
   git tag v1.0.0
   git push origin v1.0.0
   ```
   И двата файла се добавят автоматично към новото издание.

След първото успешно сглобяване на Windows в хранилището ще се появи `src-tauri/Cargo.lock`, ако го качите обратно, следващите сглобявания ще са по-бързи и предвидими.

## Версия

Сменя се на две места: `version` в `package.json` и в `src-tauri/tauri.conf.json` (и по желание в `src-tauri/Cargo.toml`).

## Дистанционно управление

- На компютъра: **Пусни → Мрежа** показва QR код и адрес. Приложението слуша на порт 8723 (или следващия свободен). Windows може да поиска разрешение за мрежата при първо включване.
- На телефона (Android приложението): **Пусни → Мрежа → Сканирай QR кода**. Двете устройства трябва да са в една Wi‑Fi мрежа.
- Без приложение: адресът от QR кода се отваря в браузъра на телефона и показва превю с бутони за управление.
- „Нов код“ прави стария адрес невалиден.

## Локално сглобяване (по желание)

Windows (нужни са Node 22, Rust и Visual Studio Build Tools):

```
npm install
bash scripts/fetch-vendor.sh
npx tauri build
```

Инсталаторът е в `src-tauri/target/release/bundle/nsis/`.

Android (нужни са Node 22, JDK 21 и Android SDK):

```
npm install
bash scripts/fetch-vendor.sh
npx cap add android
python3 scripts/android-patch.py
npx cap sync android
cd android && ./gradlew assembleDebug
```

APK е в `android/app/build/outputs/apk/debug/`.

## Бележки

- APK е „debug“ вариант: подписан е с общ ключ за разработка, инсталира се нормално (трябва да е разрешена инсталация от неизвестни източници), но не е за Google Play. За Play е нужен собствен ключ и `assembleRelease`.
- Връзката между устройствата е през http в локалната мрежа и не излиза в интернет. Адресът съдържа таен код.
- Библиотеката за `.docx` (mammoth) и за сканиране на QR код (jsQR) се свалят при сглобяване в `www/vendor/`.
