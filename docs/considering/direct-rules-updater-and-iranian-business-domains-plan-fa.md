# پلن اجرایی بهبود رول‌ها، آپدیتر، ترافیک و Tray در BiFlow

وضعیت: پیشنهادی؛ این فایل فقط پلن است و هنوز هیچ رفتار اجرایی را تغییر
نمی‌دهد.  
تاریخ بررسی: ۲۰۲۶-۰۸-۱۷  
دامنهٔ کار: Direct/VPN pins، اعمال زندهٔ رول‌ها، آپدیتر برنامه و کاتالوگ
دامنه‌های کسب‌وکارهای ایرانی، شمارندهٔ ترافیک، منوی Tray و ورودی‌های متنی

## نتیجهٔ مورد انتظار

پس از اجرای این پلن:

1. افزودن، جابه‌جایی یا حذف یک رول، بدون انتظار هم‌زمان برای DNS ذخیره شود و
   اگر Mihomo فعال است حداکثر ظرف ۵ ثانیه واقعاً روی ترافیک جاری اعمال شود؛
   هدف عملکردی P95 حداکثر ۲ ثانیه است.
2. واردکردن دامنهٔ اصلی یا هر زیر‌دامنه، کل دامنهٔ ثبت‌پذیر و همهٔ
   زیر‌دامنه‌های آن را پوشش دهد. برای نمونه هر کدام از
   `example.com`، `www.example.com` یا `api.shop.example.com` به رول واحد
   `+.example.com` تبدیل شوند.
3. هم‌زمان با بررسی یا نصب آپدیت هیچ خطای قرمز
   `an update is already in progress` دیده نشود. وضعیت مشغول از بک‌اند به UI
   برسد و تمام کنترل‌های آپدیت تا پایان همان عملیات غیرفعال بمانند.
4. دکمهٔ آپدیت یک مسیر واقعی، قابل مشاهده، قابل تکرار و قابل بازیابی از خطا
   داشته باشد؛ AppImage و NSIS خودکار به‌روزرسانی شوند و بستهٔ Debian صادقانه
   مسیر دانلود/نصب دستی را نشان دهد.
5. یک کاتالوگ نگه‌داری‌شده در خود BiFlow برای دامنه‌های فعال کسب‌وکارهای
   ایرانی غیر `.ir` وجود داشته باشد، همراه با منبع، تاریخ بازبینی، اعتبارسنجی
   و تست جلوگیری از حذف ناخواسته.
6. هیچ آپدیتی ــ چه آپدیت برنامه و چه refresh فهرست ابری ــ رول‌های سفارشی
   Direct/VPN کاربر را حذف، خالی یا با snapshot عمومی جایگزین نکند. این تضمین
   باید در ارتقای واقعی Linux و Windows اثبات شود.
7. ترافیک ورودی و خروجی دقیقاً یک بار و در جهت درست شمرده شود؛ برای نمونه
   دریافت ۱۰۰ MiB نباید حدود ۲۰۰ MiB نمایش داده شود. شمارنده متعلق به نشست
   اجرای برنامه باشد و پس از Quit و اجرای دوباره از صفر شروع شود.
8. منوی راست‌کلیک آیکون Tray گزینهٔ `Dashboard` داشته باشد؛ انتخاب آن پنجره
   را باز و foreground کند و کاربر را مستقیماً به صفحهٔ Dashboard ببرد.
9. Paste از منوی راست‌کلیک در inputهای کنترل‌شده واقعاً state برنامه را
   به‌روزرسانی کند؛ متن نباید لحظه‌ای ظاهر و با render بعدی ناپدید شود. ورودی
   رول فقط پس از ثبت موفق پاک شود و در failure برای retry باقی بماند.

## شواهد وضعیت فعلی

| موضوع             | شاهد فعلی                                                                                                                                                                                                                                                                                     | شکاف                                                                                                                                                                                  |
| ----------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| تأخیر رول         | `RuleManager::pin` در `crates/iran-split-rules/src/lib.rs` پیش از گرفتن قفل و ذخیره، برای دامنه تا `RESOLVE_BUDGET = 3s` منتظر DoH می‌ماند. در `debug.log` محلی چند `pin_route` در ۲۰۲۶-۰۸-۱۷ دقیقاً ۳۰۰۲ میلی‌ثانیه طول کشیده‌اند.                                                           | DNS باید از مسیر بحرانی حذف شود. نتیجهٔ DNS در provider دامنهٔ Mihomo استفاده نمی‌شود و نباید ذخیرهٔ رول را متوقف کند.                                                                |
| اعمال زنده        | فرمان‌های `pin_route`، `add_direct_rule` و `remove_direct_rule` فقط `direct-rules.json` را از طریق `RuleManager` تغییر می‌دهند. فایل‌های provider فقط در `prepare_runtime` پلتفرم‌ها ساخته می‌شوند.                                                                                           | رول UI با موفقیت برمی‌گردد، ولی نسل فعال Mihomo بازسازی/Reload نمی‌شود و تا راه‌اندازی بعدی لزوماً روی ترافیک اعمال نشده است.                                                         |
| دامنه و زیر‌دامنه | `RuleSet::decide` برای رول سفارشی از برابری دقیق استفاده می‌کند، ولی فهرست ایران suffix-match است. ADR 0034 هم exact-host را عمداً تثبیت کرده است.                                                                                                                                            | رفتار سفارشی باید به registrable-domain + suffix تغییر کند و تشخیص داخلی، mock و Mihomo یک قرارداد واحد داشته باشند.                                                                  |
| قفل آپدیت         | `UpdateCoordinator::begin` از `try_lock` استفاده می‌کند، اما بررسی پس‌زمینه وضعیت busy را به صفحهٔ About اعلام نمی‌کند.                                                                                                                                                                       | کاربر می‌تواند هنگام در اختیار بودن قفل کلیک کند؛ خطای داخلی قفل به پیام قرمز تبدیل می‌شود.                                                                                           |
| خطای واقعی آپدیت  | در `debug.log`، بررسی پس‌زمینه در ۲۰۲۶-۰۸-۱۷ هر چهار تلاش شبکه را با `update.check_attempt_failed` از دست داده و کلیک دستی ساعت ۰۸:۳۴:۳۰ فوراً با `an update is already in progress` شکست خورده است.                                                                                          | وضعیت عملیات پس‌زمینه باید قابل مشاهده باشد و شبکه مسیر proxy-aware و direct fallback قابل آزمون داشته باشد.                                                                          |
| نصب آپدیت         | `perform_complete_update_install` وضعیت را بررسی می‌کند و `perform_signed_update_install` دوباره retry-check و سپس یک `check()` دیگر اجرا می‌کند. Stack نیز پیش از دانلود pause می‌شود.                                                                                                       | یک نصب ممکن است چند بار به GitHub وابسته شود و پس از قطع مسیر شبکه شکست بخورد. دانلود و امضاسنجی باید پیش از pause انجام شود و فقط یک candidate استفاده شود.                          |
| فهرست ایران       | `resources/rules/iran-domains.txt` اکنون ۶۲٬۸۲۸ ورودی دارد و `+.ir` را شامل می‌شود.                                                                                                                                                                                                           | همهٔ `.ir`ها از قبل پوشش دارند؛ خلأ واقعی، دامنه‌های ایرانی فعال روی TLDهای دیگری مانند `.com`، `.io`، `.app` و `.cloud` است.                                                         |
| شمارندهٔ ترافیک   | `get_traffic_totals` snapshot تجمعی Mihomo را به `traffic::accumulate` می‌دهد و نتیجه را در `traffic-totals.json` برای اجراهای بعد نگه می‌دارد. ADR 0043 نیز lifetime total را تصمیم پذیرفته‌شده می‌داند.                                                                                     | خواستهٔ جدید session-based است. نمونه‌برداری تکراری یا reset/reconnect هسته نباید یک بایت را دوباره fold کند و اجرای تازهٔ برنامه نباید total قبلی را بارگذاری کند.                   |
| منوی Tray         | کلیک چپ Tray فقط `show_main` را اجرا می‌کند. `build_tray_menu` و `handle_tray_menu` actionهای lifecycle و Quit را دارند، اما action مستقلی برای Dashboard و تغییر صفحهٔ React وجود ندارد.                                                                                                     | راست‌کلیک باید Dashboard را همیشه در دسترس بگذارد و علاوه بر show/focus، route/page فعال را به Dashboard تغییر دهد.                                                                   |
| Paste در input    | `InputContextMenu.replaceRange` مستقیماً `field.value` را تغییر می‌دهد و `input` dispatch می‌کند. تست فعلی فقط `defaultValue` و input کنترل‌نشده را می‌آزماید. فرم Direct Rules نیز بعد از resolve شدن `addRule` متن را پاک می‌کند، درحالی‌که store خطا را catch و Promise را resolve می‌کند. | mutation مستقیم DOM می‌تواند tracker/state ورودی کنترل‌شدهٔ React را دور بزند؛ render بعدی مقدار قدیمی را برمی‌گرداند. failure ثبت رول نیز اکنون از success برای فرم قابل تشخیص نیست. |

`debug.log` فقط برای این ارزیابی خوانده شد و نباید وارد Git شود.

## تصمیم‌های رفتاری پیش از پیاده‌سازی

### ۱. معنی «دامنه و برعکس»

- ورودی دامنه با یک snapshot محلی از Public Suffix List به دامنهٔ ثبت‌پذیر
  (eTLD+1) تبدیل شود؛ حذف سادهٔ اولین label مجاز نیست.
- `api.example.co.uk` باید به `example.co.uk` برسد، نه `co.uk`.
- بخش private فهرست suffix نیز فعال باشد تا برای نمونه `user.github.io` باعث
  مستقیم‌شدن کل `github.io` نشود.
- دامنهٔ Unicode ابتدا به IDNA ASCII نرمال شود و سپس ریشهٔ ثبت‌پذیر محاسبه
  شود. IP همچنان exact باقی بماند.
- Direct pin و VPN pin هر دو همین semantics را داشته باشند. بنابراین جابه‌جایی
  یک زیر‌دامنه به VPN، کل ریشه و زیر‌دامنه‌های آن را جابه‌جا می‌کند و رول‌های
  متناقض parent/child ایجاد نمی‌شوند.
- UI صریحاً متن «این رول دامنهٔ اصلی و تمام زیر‌دامنه‌ها را پوشش می‌دهد» را
  نشان دهد. استثنای مستقل برای یک زیر‌دامنه در این تغییر پشتیبانی نمی‌شود.

### ۲. قرارداد واحد تطبیق

- شکل canonical ذخیره‌شده در `direct-rules.json` فقط ریشهٔ ثبت‌پذیر باشد.
- شکل provider دامنه `+.example.com` باشد تا ریشه و زیر‌دامنه‌ها را پوشش دهد.
- `RuleSet::decide`، فایل‌های تولیدشدهٔ Mihomo، mock API، Diagnostics و UI از
  یک تابع canonicalization و یک قاعدهٔ suffix-match پیروی کنند.
- `resolved_ips` مبنای تصمیم‌گیری مسیر نباشد؛ آدرس CDN یا هاست اشتراکی نباید
  با افزودن یک دامنه به‌طور ناخواسته مستقیم شود. اگر این فیلد برای نمایش باقی
  بماند، فقط metadata تشخیصی و refresh پس‌زمینه باشد.

### ۳. اعمال زنده باید تراکنشی باشد

- موفقیت فرمان رول به معنی «ذخیره و اعمال‌شده» باشد، نه فقط نوشته‌شدن JSON.
- وقتی Stack متوقف است، persist کافی است و شروع بعدی همان revision را مصرف
  می‌کند.
- وقتی Stack running/degraded است، تغییر رول باید با lifecycle lock هماهنگ
  شود، نسل immutable جدید بسازد، با Mihomo اعتبارسنجی کند، از مرز helper ثبت
  کند، روی کنترلر Hot Reload شود و آمادگی providerها دوباره تأیید شود.
- اگر Reload یا readiness شکست خورد، سند قبلی و نسل فعال قبلی بازگردانده شوند
  و UI یک خطای قابل اقدام بگیرد. وضعیت نیمه‌ذخیره‌شده یا «بعداً شاید اعمال
  شود» قابل قبول نیست.
- برای helper یک قرارداد محدود و audit‌شده جهت فعال‌کردن نسل ثبت‌شده لازم
  است؛ مسیر دلخواه، URL، secret یا محتوای خام کاربر نباید وارد لاگ شود.

### ۴. آپدیتر یک state machine واحد دارد

- `UpdateCoordinator` علاوه بر mutex، snapshot قابل خواندن شامل
  `operation_id`، `initiator`، phase، درصد، نسخه، کانال‌ها و خطای نهایی نگه
  دارد.
- phaseها حداقل شامل `idle`، `checking`، `available`، `downloading`،
  `verifying`، `installing`، `restarting`، `current`، `manual` و `failed`
  باشند.
- bootstrap و یک فرمان `get_update_state` وضعیت جاری را برگردانند و همهٔ
  تغییرها با event دارای همان `operation_id` منتشر شوند. UI event قدیمی یک
  عملیات قبلی را نپذیرد.
- کلیک تکراری در بک‌اند idempotent باشد: snapshot/شناسهٔ عملیات موجود را
  برگرداند، نه error. UI نیز قبل از رسیدن درخواست دوم همهٔ دکمه‌ها را disable
  کند.
- بررسی پس‌زمینه می‌تواند بدون بنر خطا شکست بخورد، اما هنگام اجرای آن وضعیت
  busy باید به UI برسد. عبارت `an update is already in progress` هرگز متن
  کاربرپسند نیست.
- در هر phase فقط action مرتبط نمایش داده شود: Check در idle/current، Install
  در available و Retry در failed. اگر کنترلی حین busy روی صفحه باقی می‌ماند،
  غیرفعال باشد.

### ۵. رول‌های سفارشی دادهٔ پایدار کاربر هستند

- `direct-rules.json` و هر backup/migration آن فقط در data directory همان کاربر
  نگه‌داری شود و هرگز جزو فایل‌های قابل جایگزینی package، bundled resources یا
  cloud-rule cache نباشد.
- آپدیت snapshot ابری فقط staging/cache مربوط به providerهای عمومی را publish
  کند و حق نوشتن، truncate، rename یا delete کردن `direct-rules.json` را نداشته
  باشد.
- ارتقای AppImage، Debian و NSIS باید data directory موجود را حفظ کند. اسکریپت
  upgrade/uninstall نباید رول سفارشی را پاک کند؛ حذف دادهٔ کاربر فقط با action
  صریح و جداگانهٔ کاربر مجاز است.
- هر تغییر schema برای رول‌های قدیمی، ابتدا از فایل موجود یک last-good backup
  اتمیک بسازد و مقدار رول را در لاگ ثبت نکند. migration باید Direct/VPN،
  timestampها و revision را حفظ کند و فقط normalization ضروری را انجام دهد.
- اگر migration یا load شکست خورد، آپدیت/راه‌اندازی نباید با سند خالی ادامه
  دهد. فایل قبلی دست‌نخورده بماند، backup قابل بازیابی باشد و خطای قابل اقدام
  نمایش داده شود.
- مسیرهای Linux و Windows از یک قرارداد persistence و fixture مشترک استفاده
  کنند؛ تفاوت path separator یا installer نباید رفتار دادهٔ کاربر را تغییر
  دهد.

### ۶. ترافیک، شمارندهٔ نشست اجرای برنامه است

- منبع authoritative فقط totalهای cumulative کنترلر Mihomo باشد. دادهٔ یکسان
  از Hiddify، رابط TUN، سیستم‌عامل یا چند endpoint با هم جمع نشود.
- قرارداد جهت‌ها صریح باشد: `uploadTotal` فقط ترافیک خروجی/sent و
  `downloadTotal` فقط ترافیک ورودی/received است. UI نیز این دو را جابه‌جا یا
  برای نمایش یک عدد با هم جمع نکند.
- هر snapshot cumulative نسبت به آخرین snapshot همان generation به delta
  تبدیل شود. polling مجدد یک snapshot یکسان باید delta صفر بسازد، نه اینکه
  total قبلی را دوباره اضافه کند.
- هنگام restart/reset شدن Mihomo، accumulator درون همان اجرای برنامه مقدار
  آخر generation قبلی را دقیقاً یک بار تثبیت و generation جدید را از baseline
  تازه دنبال کند. کاهش counter، wrap یا controller جدید نباید عدد منفی یا
  دوبرابر ایجاد کند.
- عمر accumulator برابر عمر process دسکتاپ باشد. در launch تازه مقدار صفر است
  و lifetime total قبلی از `traffic-totals.json` خوانده نمی‌شود. persistence
  فعلی با migration صریح بازنشسته شود تا فایل legacy روی اولین اجرا باعث
  نمایش عدد قدیمی نشود.
- بستن پنجره به Tray خروج برنامه نیست و نباید شمارنده را صفر کند. فقط Quit
  واقعی، پایان process و اجرای دوباره یک نشست تازه با مقدار صفر می‌سازد.
- arithmetic با byte صحیح و checked/saturating انجام شود؛ تبدیل به KiB/MiB فقط
  در لایهٔ نمایش باشد و تست‌ها tolerance را صرفاً برای ترافیک سربار واقعی
  تعریف کنند، نه برای خطای دوبرابرشماری.

### ۷. Dashboard یک action مستقل در منوی Tray است

- یک آیتم با شناسهٔ پایدار `dashboard` در ابتدای context menu و پیش از actionهای
  اتصال قرار گیرد و با separator از آن‌ها جدا شود.
- Dashboard حتی هنگام start/stop/update یا lifecycle busy فعال بماند؛ این action
  فقط ناوبری UI است و نباید به وضعیت اتصال وابسته باشد.
- handler ابتدا پنجرهٔ اصلی را show، unminimize و focus کند و سپس با یک event یا
  command typed، صفحهٔ فعال store را به `dashboard` ببرد. فقط بازکردن آخرین
  صفحهٔ قبلی کافی نیست.
- در Basic و Advanced mode همان Dashboard متناظر با mode جاری نمایش داده شود.
  اگر frontend هنوز آماده نیست، event ناوبری باید پس از ready شدن تحویل یا با
  state قابل query بازیابی شود.
- کلیک چپ فعلی Tray می‌تواند فقط پنجره را باز کند؛ قرارداد action صریح
  Dashboard در راست‌کلیک مستقل و قابل تست باقی بماند.
- action و نتیجهٔ show/focus/navigation با فیلدهای structured و بدون دادهٔ
  کاربر در `debug.log` ثبت شوند؛ failure فوکوس یا event نادیده گرفته نشود.

### ۸. Paste باید state ورودی کنترل‌شده را تغییر دهد

- helper مشترک Cut/Paste نباید فقط DOM property را mutate کند. جایگزینی متن
  باید از native value setter سازگار با React و یک `InputEvent` bubbling با
  `inputType` درست عبور کند تا `onChange` و state مالک input نیز به‌روز شوند.
- متن clipboard دقیقاً در selection فعلی درج شود؛ اگر بخشی انتخاب شده همان بخش
  جایگزین و در غیر این صورت متن در caret وارد شود. caret پس از متن جدید قرار
  گیرد و مقدار در renderهای بعدی پایدار بماند.
- input و textarea کنترل‌شده و کنترل‌نشده هر دو پشتیبانی شوند. fieldهایی مانند
  number که Selection API کامل ندارند نباید exception بدون رسیدگی بسازند.
- target و selection عملیات async paste باید مشخص باشند. اگر input پیش از
  پایان `clipboard.readText()` unmount/disabled شد، عملیات بدون تغییر input
  دیگری متوقف و یک خطای قابل فهم نمایش داده شود.
- clipboard permission/read failure مقدار قبلی را پاک نکند. محتوای clipboard
  یا input در tracing نوشته نشود.
- فرم Direct Rules فقط پس از پاسخ موفق backend ورودی را پاک کند. validation،
  revision conflict، timeout یا apply failure باید مقدار paste/type‌شده را
  برای اصلاح یا Retry نگه دارد؛ store باید success/failure را به caller منتقل
  کند، نه اینکه هر دو مسیر را با Promise موفق یکسان کند.

## ترتیب پیاده‌سازی

### فاز ۰: بازتولید کنترل‌شده و baseline

1. پیش از تغییر Rust دوباره `debug.log` جاری را بخوان و timeline رول و آپدیت
   را با `trace_id` ثبت کن؛ فایل تولیدشده commit نشود.
2. نوع بستهٔ در حال اجرا را مشخص کن: AppImage، Debian، NSIS یا portable.
   انتظار self-update برای Debian نباید با AppImage یکی باشد.
3. روی نسخهٔ فعلی این سناریوها را ثبت کن:
   - افزودن domain در حالت running و اندازه‌گیری زمان پاسخ و زمان تغییر route؛
   - افزودن root و تست یک subdomain؛
   - هم‌زمانی background check و کلیک دستی؛
   - check موفق، download ناقص، signature failure و install failure؛
   - تزریق snapshotهای ترافیک یکسان، disconnect/reconnect و restart شدن Mihomo؛
   - دریافت کنترل‌شدهٔ ۱۰۰ MiB و مقایسهٔ sent/received UI با total کنترلر؛
   - راست‌کلیک Tray از صفحه‌ای غیر از Dashboard و در حالت پنجرهٔ مخفی؛
   - Paste از منوی سفارشی روی input کنترل‌شدهٔ Direct Rules و Diagnostics، سپس
     ایجاد چند render بدون submit برای مشاهدهٔ پایداری مقدار.
4. قرارداد Release را بررسی کن: `latest.json`، URLهای artifact، signature،
   version و سازگاری platform/arch. مقدار URL یا token کامل در لاگ نوشته نشود.

خروجی فاز: تست‌های قرمز کوچک و قطعی برای همهٔ باگ‌ها، نه صرفاً اسکرین‌شات.

### فاز ۱: canonical domain و suffix matching

1. در `iran-split-rules` یک تابع خالص برای normalize + registrable root اضافه
   کن و Public Suffix List را به‌صورت version-pinned و آفلاین مصرف کن.
2. migration هنگام load بنویس تا رول‌های قدیمی exact به ریشهٔ canonical تبدیل،
   duplicateها ادغام و revision فقط یک بار افزایش یابد.
3. `RuleManager::pin/remove` و `RuleSet::decide` را برای semantics جدید هماهنگ
   کن؛ Direct و VPN mutual exclusion را بعد از canonicalization اعمال کن.
4. writerهای Linux/Windows و `iran-split-mihomo` دامنه‌های سفارشی را با `+.`
   تولید کنند. literal IPها بدون تغییر بمانند.
5. mock و متن صفحهٔ Direct rules را با همین قرارداد به‌روز کن.

معیار پذیرش فاز:

- افزودن `api.shop.example.com` فقط یک رکورد `example.com` می‌سازد.
- `example.com`، `www.example.com` و `api.shop.example.com` یک outbound دارند.
- `notexample.com` match نمی‌شود.
- `user.github.io` کل `github.io` را match نمی‌کند.
- انتقال هر کدام از شکل‌های دامنه به VPN همان رکورد canonical را جابه‌جا می‌کند.

### فاز ۲: حذف تأخیر و اعمال واقعی روی Mihomo فعال

1. DoH را از مسیر synchronous `pin` بردار؛ persist دامنه به DNS وابسته نباشد.
2. یک operation مخصوص rule-apply در core/backend تعریف کن تا با
   start/stop/pause/reconcile رقابت نکند.
3. تولید نسل، validation، helper registration، Hot Reload و readiness را در
   Linux و Windows با ترتیب یکسان پیاده کن.
4. نسل قبلی را تا موفقیت کامل نگه دار و rollback سند + runtime را تست کن.
5. پس از موفقیت، snapshot provider count/rules loaded و وضعیت UI را refresh کن.
6. برای شروع، پایان، timeout، rollback و failure رویدادهای structured با
   `event`، `section`، `initiator`، `cause`، `trace_route` و `trace_id` بنویس؛
   مقدار دامنه یا rule خام log نشود.

معیار پذیرش فاز:

- روی Stack سالم و running، مسیر root و subdomain حداکثر ظرف ۵ ثانیه تغییر
  کند و restart کامل Hiddify لازم نباشد.
- UI پیش از readiness موفق عملیات را تمام‌شده نشان ندهد.
- failure در validation/controller/provider سند و runtime قبلی را سالم نگه
  دارد.
- تغییر رول هنگام lifecycle operation یا serialize می‌شود یا خطای retryable
  مشخص می‌دهد؛ deadlock و state نیمه‌کاره وجود ندارد.

### فاز ۳: بازطراحی مسیر آپدیت

1. `UpdateCoordinator` را به state machine قابل اشتراک با UI تبدیل کن و
   background/manual/install را از همان مسیر عبور بده.
2. عملیات install فقط یک بار update candidate را بگیرد. sidecar status و
   candidate برنامه در یک نتیجهٔ منسجم جمع شوند؛ سه `check()` فعلی حذف شوند.
3. برای check و download ابتدا client معمول proxy-aware و در خطا یک تلاش
   bounded با `no_proxy` انجام بده. retry فقط برای خطاهای شبکه‌ای مجاز باشد،
   نه manifest/signature نامعتبر.
4. از API جداگانهٔ `Update::download` و `Update::install` استفاده کن:
   - دانلود، timeout، progress و signature verification در حالی انجام شود که
     اتصال فعلی برقرار است؛
   - بعد از دریافت bytes معتبر، Stack برای نصب pause شود؛
   - سپس install و restart انجام شود.
5. یک guard وضعیت قبلی اتصال را نگه دارد. اگر نصب پیش از خروج برنامه شکست
   خورد، Stack به وضعیت قبلی برگردد و کنترل‌ها آزاد شوند.
6. پیش از شروع نصب، وجود و خوانایی سند رول سفارشی بررسی و fingerprint امن آن
   بدون ثبت محتوا نگه‌داری شود. بعد از relaunch همان سند یا migration موفق آن
   باید موجود باشد؛ نبودن آن failure ارتقا محسوب شود، نه حالت first-run.
7. خطای rule sidecar، Mihomo sidecar و app package جداگانه ثبت و نمایش داده
   شوند. موفقیت بخشی نباید failure بخش دیگر را پنهان کند.
8. Debian به phase `manual` برود، Release رسمی را باز کند و هرگز progress
   ساختگی «نصب شد» نشان ندهد. AppImage و NSIS مسیر امضاشدهٔ خودکار داشته
   باشند.
9. UI About فقط از snapshot آپدیتر render شود. error مربوط به busy suppress
   نشود؛ اساساً از بک‌اند تولید نشود. خطاهای واقعی شبکه/manifest/signature با
   Retry فعال پس از آزادشدن operation نشان داده شوند.

معیار پذیرش فاز:

- background check کنترل‌ها را disable می‌کند و هیچ بنر قرمز busy نمی‌سازد.
- double-click یا Check + Retry هم‌زمان فقط یک فراخوانی شبکه ایجاد می‌کند.
- timeout همهٔ دکمه‌ها را دوباره فعال و phase را نهایی می‌کند؛ lock نشت نمی‌کند.
- download failure اتصال جاری را قطع نمی‌کند.
- install failure پس از pause، در صورت زنده‌ماندن برنامه اتصال را restore
  می‌کند.
- رول‌های سفارشی Direct و VPN بعد از ارتقای AppImage، Debian و NSIS با همان
  outbound و بدون duplicate در دسترس‌اند.
- AppImage/NSIS از یک Release امضاشده به نسخهٔ بعد می‌روند و پس از relaunch
  نسخهٔ جدید گزارش می‌شود.

### فاز ۴: کاتالوگ دامنه‌های کسب‌وکارهای ایرانی

#### ساختار پیشنهادی

- فایل مستقل `resources/rules/iran-business-domains.txt` ایجاد شود. snapshot
  upstream فعلی دست‌نخورده بماند تا provenance و مجوز آن مخدوش نشود.
- هر خط runtime به شکل `+.domain.tld` باشد.
- metadata منبع در فایل جداگانهٔ
  `resources/rules/iran-business-domains.sources.json` نگه‌داری شود و حداقل
  شامل `domain`، `category`، `official_url`، `discovered_from`،
  `verified_at` و `status` باشد.
- `scripts/sync-rules.mjs` و manifest، provider جدید را با hash و entry count
  اعتبارسنجی کنند، ولی آپدیت upstream نتواند کاتالوگ دستی را overwrite کند.
- provider جدید در embedded snapshot، resource mapping، CloudRuleStore،
  RuntimePaths، Mihomo config، هر دو platform backend و allowlist فایل‌های
  helper ثبت شود.
- shared CDN، analytics، payment gateway عمومی یا دامنه‌ای که مالکیت آن روشن
  نیست به‌خاطر استفادهٔ یک کسب‌وکار مستقیم نشود. فقط root رسمی همان کسب‌وکار
  و alias اول‌شخص مستند پذیرفته شود.
- همهٔ دامنه‌های `.ir` به‌علت وجود `+.ir` از افزودن تکراری حذف شوند.

#### موج اول دامنه‌های تحقیق‌شده

این دامنه‌ها در ۲۰۲۶-۰۸-۱۷ از صفحهٔ رسمی فعال تأیید شدند و در snapshot فعلی
`iran-domains.txt` ورودی root/suffix متناظر ندارند. هنگام اجرا یک بررسی نهایی
مالکیت/redirect نیز انجام شود.

| دسته           | کسب‌وکار    | ورودی پیشنهادی       | منبع رسمی                                     |
| -------------- | ----------- | -------------------- | --------------------------------------------- |
| فروشگاه        | تکنولایف    | `+.technolife.com`   | [technolife.com](https://www.technolife.com/) |
| لندتک          | ازکی‌وام    | `+.azkivam.com`      | [azkivam.com](https://azkivam.com/)           |
| سرمایه‌گذاری   | ازکی سرمایه | `+.azkisarmayeh.com` | [azkisarmayeh.com](https://azkisarmayeh.com/) |
| پرداخت         | نکست‌پی     | `+.nextpay.com`      | [nextpay.com](https://www.nextpay.com/)       |
| پرداخت         | پی‌پینگ     | `+.payping.io`       | [payping.io](https://payping.io/)             |
| پرداخت         | تومن        | `+.tomanpay.com`     | [tomanpay.com](https://tomanpay.com/)         |
| دارایی دیجیتال | کیف پول من  | `+.kifpool.me`       | [kifpool.me](https://kifpool.me/)             |
| سفر            | سفرمارکت    | `+.safarmarket.com`  | [safarmarket.com](https://safarmarket.com/)   |
| زیرساخت ابری   | ابرآراز     | `+.arazcloud.com`    | [arazcloud.com](https://arazcloud.com/)       |
| دارایی دیجیتال | اکسکوینو    | `+.excoino.com`      | [excoino.com](https://excoino.com/)           |
| دارایی دیجیتال | هیتوبیت     | `+.hitobit.com`      | [hitobit.com](https://hitobit.com/fa)         |
| کاریابی        | کاربوم      | `+.karboom.io`       | [karboom.io](https://karboom.io/)             |
| فین‌تک         | اوانو       | `+.ewano.app`        | [ewano.app](https://ewano.app/)               |

منابع discovery و cross-check:

- [Tehran Index: E-commerce](https://tehranindex.com/sectors/ecommerce)
- [Tehran Index: Fintech](https://tehranindex.com/sectors/fintech)
- [Tehran Index: Digital Assets](https://tehranindex.com/sectors/crypto)
- [Tehran Index: Cloud](https://tehranindex.com/sectors/cloud)

معیار پذیرش فاز:

- `pnpm rules:check` hash، count، syntax، uniqueness و metadata همهٔ ورودی‌ها
  را آفلاین تأیید کند.
- `www.technolife.com` و `selleracademy.technolife.com` در mock و runtime واقعی
  DIRECT باشند؛ یک دامنهٔ مشابه نامرتبط DIRECT نشود.
- refresh ابری failure-safe باشد و آخرین snapshot سالم را نگه دارد.
- refresh ابری هیچ تغییری در محتوا، revision یا timestamp فایل رول‌های سفارشی
  ایجاد نکند.
- حذف یا تغییر بیش از آستانهٔ مشخص در کاتالوگ نیازمند review صریح باشد.

### فاز ۵: اصلاح ترافیک، Dashboard در Tray و Paste ورودی

1. API داخلی traffic را از lifetime persistence به یک accumulator درون‌حافظه‌ای
   و session-scoped تغییر بده؛ ownership آن در state دسکتاپ باشد تا هر launch
   نمونهٔ تازه و صفر داشته باشد.
2. به‌جای fold کردن snapshot کامل در هر poll، برای هر generation فقط delta
   نسبت به نمونهٔ قبلی را اضافه کن. disconnect، snapshot تکراری، counter reset
   و restart شدن controller را به‌عنوان transitionهای صریح مدل کن.
3. خواندن و نوشتن `traffic-totals.json` را از مسیر عادی حذف کن. وجود فایل از
   نسخهٔ قدیمی نباید startup را خراب یا total را بازیابی کند؛ cleanup احتمالی
   باید محدود، recoverable و دارای تست migration باشد.
4. نگاشت `uploadTotal`/`downloadTotal` تا `sent`/`received` و labelهای UI را
   end-to-end بررسی کن. یک منبع authoritative انتخاب شود و هیچ total واسطی
   دوباره با آن جمع نشود.
5. آیتم `Dashboard` را به builder منوی Tray و handler آن اضافه کن؛ action باید
   در هر snapshot اتصال موجود و فعال باشد.
6. یک پیام typed برای navigate-to-dashboard تعریف کن. frontend آن را به
   `setPage("dashboard")` وصل کند و backend show/unminimize/focus را پیش از
   navigation انجام دهد.
7. برای sample، generation transition، reset نشست، انتخاب Dashboard و خطاهای
   window/navigation tracing ساختاریافته با `trace_id` اضافه کن.
8. `replaceRange` منوی input را به helper سازگار با controlled React fields
   تبدیل کن؛ native setter، `InputEvent`، selection replacement و caret را برای
   input/textarea و Cut/Paste یکسان مدیریت کن.
9. قرارداد `addRule` در store/component را طوری تغییر بده که success قابل تشخیص
   باشد؛ ورودی فقط پس از persist/apply موفق خالی شود و در همهٔ خطاها باقی
   بماند.
10. خطای clipboard را در سطح UI به‌صورت non-destructive و قابل اقدام نشان بده؛
    مقدار clipboard یا input در log و error telemetry قرار نگیرد.

معیار پذیرش فاز:

- سه بار poll کردن snapshot یکسان هیچ بایتی به total اضافه نمی‌کند.
- workload کنترل‌شدهٔ ۱۰۰ MiB دانلود، تقریباً ۱۰۰ MiB به received اضافه می‌کند؛
  sent فقط سربار واقعی خروجی را دارد و total ورودی حدود ۲۰۰ MiB نمی‌شود.
- disconnect/reconnect یا restart شدن Mihomo در یک اجرای برنامه، بایت‌های قبلی
  را نه حذف و نه دوباره‌شماری می‌کند.
- مخفی و دوباره بازکردن پنجره از Tray total را نگه می‌دارد؛ Quit و launch تازه
  هر دو counter را از صفر آغاز می‌کند.
- `Dashboard` در context menu، حتی هنگام lifecycle busy، فعال است. انتخاب آن از
  Settings/About و پنجرهٔ hidden/minimized، پنجره را باز و focus و صفحهٔ
  Dashboard را فعال می‌کند.
- Paste در input خالی، وسط متن و روی selection مقدار state را همان‌طور که در
  DOM دیده می‌شود تغییر می‌دهد و پس از renderهای بعدی، تغییر صفحهٔ فرعی یا
  تغییر `actionPending` ناپدید نمی‌شود.
- ثبت موفق رول input را یک بار پاک می‌کند؛ validation/backend/apply failure متن
  را بدون تغییر برای Retry نگه می‌دارد.
- clipboard denial یا unmount شدن target هیچ input دیگری را تغییر یا پاک
  نمی‌کند و unhandled rejection ندارد.

### فاز ۶: تست، مستندات، نسخه و انتشار

#### تست‌های واحد Rust

- IDNA، eTLD+1، suffix خصوصی، `co.uk`، دامنهٔ تک-label نامعتبر، IP و مرز label.
- migration رول قدیمی، duplicate parent/child، revision conflict و mutual
  exclusion بین Direct/VPN.
- persistence upgrade با fixture واقعی `direct-rules.json`: آپدیت app، refresh
  ابری و migration موفق باید Direct/VPN و revision را روی مسیرهای Linux و
  Windows حفظ کنند؛ migration ناموفق باید فایل قبلی را سالم بگذارد.
- تصمیم root/subdomain/sibling و عدم match روی `notexample.com`.
- provider writer با `+.` در `iran-split-mihomo` و هر دو platform backend.
- rule apply در حالت stopped/running، timeout، rollback و تداخل lifecycle.
- state machine آپدیتر، dedup، stale operation event، timeout، direct fallback
  و آزادشدن lock در همهٔ exit pathها.
- ترتیب updater: check یک بار، download و verify پیش از pause، install پس از
  pause و resume روی failure.
- accumulator ترافیک: snapshot تکراری، افزایش مستقل upload/download، reset و
  restart generation، overflow و launch با وجود فایل legacy.
- Tray menu builder: وجود و ترتیب Dashboard، فعال‌بودن در تمام lifecycle stateها
  و handler موفق/ناموفق show، focus و navigation.

#### تست‌های UI و e2e

- `InputContextMenu` با یک harness واقعاً controlled تست شود، نه فقط
  `defaultValue`: Paste/Cut، جایگزینی selection، caret، render مجدد، clipboard
  failure و unmount async پوشش داده شوند.
- Direct Rules: Paste پایدار بماند، submit موفق input را پاک کند و submit ناموفق
  همان متن را نگه دارد. همین پایداری روی target کنترل‌شدهٔ Diagnostics نیز
  assertion داشته باشد.
- store: eventهای background busy، جلوگیری از double submit، نپذیرفتن event
  قدیمی و نگه‌داشتن channel flags در تمام phaseها.
- About: نبود پیام busy، disabled بودن همهٔ کنترل‌ها در busy، فقط action مرتبط
  در هر phase و Retry فقط پس از failure واقعی.
- primary e2e رول: هنگام connected یک subdomain اضافه شود و root، sibling و
  nested subdomain بدون restart کامل DIRECT شوند؛ سپس همان root به VPN منتقل
  و دوباره تست شود.
- primary e2e آپدیت: background/manual overlap، download progress، failure،
  retry، موفقیت و relaunch mock.
- packaged-upgrade e2e: پیش از ارتقا یک Direct pin و یک VPN pin ساخته شود؛ پس
  از ارتقای واقعی AppImage/NSIS و restart، هر دو در UI و Diagnostics با همان
  outbound دیده شوند. smoke test Debian نیز حفظ data directory را اثبات کند.
- primary e2e کاتالوگ: یک دامنهٔ غیر `.ir` از موج اول و زیر‌دامنه‌اش DIRECT
  باشند.
- primary e2e ترافیک: mock snapshotهای دقیق و تکراری تزریق شوند و جهت‌ها، عدم
  دوبرابرشماری و صفرشدن store در bootstrap تازه بررسی شود.
- تست native/packaged ترافیک: دانلود کنترل‌شده با total کنترلر مقایسه و Quit +
  relaunch واقعی صفرشدن را اثبات کند؛ close-to-tray نباید reset شود.
- تست Tray در سطح Rust/menu contract و یک smoke test native انجام شود؛ Playwright
  به‌تنهایی نمی‌تواند context menu سیستم‌عامل را با اطمینان کلیک کند.

#### مستندات و ADR

- ADR 0034 برای تغییر exact-host به registrable-domain suffix به‌روزرسانی شود.
- ADR 0040 برای state machine، dedup و ترتیب download-before-pause به‌روزرسانی
  شود.
- ADR 0043 از lifetime total به app-session total تغییر کند و migration فایل
  legacy را ثبت کند؛ تصمیم قبلی persistence صریحاً supersede شود.
- ADR 0049 برای action همیشه‌فعال Dashboard و قرارداد show/focus/navigation
  به‌روزرسانی شود.
- ADR 0048 قرارداد controlled input، native setter/event و حفظ مقدار در failure
  را ثبت کند.
- یک ADR جدید برای کاتالوگ curated جدا از snapshot دارای مجوز upstream و مرز
  helper/runtime generation نوشته شود؛ `docs/adr/README.md` نیز به‌روز شود.
- README/SNAPSHOT نحوهٔ منبع‌یابی، review و حذف دامنهٔ منقضی را توضیح دهند.
- پس از تکمیل رفتار، نسخهٔ root یک patch افزایش یابد و فقط با
  `pnpm version:sync` به manifestها منتقل شود.

## گیت نهایی پیشنهادی برای اجرای این پلن

چون این تغییر هم frontend و هم چند crate و هر دو پلتفرم را درگیر می‌کند، پس
از تکمیل همهٔ فازها یک بار گیت incremental کامل اجرا شود:

```text
cargo fmt --all --check
pnpm test
pnpm check
pnpm build
pnpm rules:check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
pnpm test:e2e
pnpm github:action-test
```

علاوه بر آن، Windows-gated code با `cargo xwin clippy/test` طبق قرارداد فعلی
CI اثبات شود و یک GitHub packaging dry-run واقعی برای AppImage، Debian و NSIS
سبز باشد. هیچ failure یا warning از کد پروژه قابل قبول نیست.

## ترتیب تحویل کم‌ریسک

1. تست‌های قرمز + تصمیم دامنه/PSL و ADR.
2. canonicalization و suffix matching بدون اعمال زنده.
3. نسل جدید و اعمال تراکنشی رول روی Linux/Windows.
4. state machine و UI آپدیتر.
5. اصلاح دانلود/نصب و smoke test بسته‌های امضاشده.
6. provider curated و موج اول دامنه‌ها.
7. accumulator ترافیک، action Dashboard در Tray و اصلاح controlled Paste.
8. e2e نهایی، مستندات، patch version و گیت کامل.

هر مرحله باید تست‌های خودش را سبز تحویل دهد؛ مرحلهٔ بعد نباید برای پنهان‌کردن
شکست مرحلهٔ قبل استفاده شود.

## ریسک‌ها و کنترل‌ها

| ریسک                                                       | کنترل                                                                                                                |
| ---------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| مستقیم‌شدن بیش از حد یک suffix عمومی یا سرویس multi-tenant | استفاده از PSL با private suffix، تست `github.io` و ممنوعیت fallback با حذف label ساده                               |
| اختلاف بین Diagnostics و ترافیک واقعی                      | حذف resolved IP از تصمیم دامنه و استفاده از canonicalization مشترک                                                   |
| Reload ناقص و ناهماهنگی helper با controller               | نسل immutable، ثبت/فعال‌سازی audit‌شده، readiness و rollback به نسل قبلی                                             |
| قطع اینترنت وسط آپدیت                                      | دانلود و امضاسنجی پیش از pause، timeout و direct fallback                                                            |
| گیرکردن همیشگی UI در busy                                  | state نهایی در guard/finally، snapshot قابل query و تست همهٔ exit pathها                                             |
| حذف رول سفارشی در app/cloud update                         | جداسازی data directory از package/cache، last-good backup، migration اتمیک و packaged-upgrade test روی Linux/Windows |
| خراب‌شدن provenance فهرست upstream                         | provider curated و metadata جدا؛ عدم ویرایش دستی snapshot دانلودشده                                                  |
| قدیمی یا واگذارشدن یک دامنهٔ کسب‌وکار                      | منبع رسمی، `verified_at`، بازبینی دوره‌ای، status و review انسانی پیش از حذف/تغییر                                   |
| دوبرابرشماری snapshot تجمعی Mihomo                         | accumulator مبتنی بر delta و generation، polling idempotent و تست snapshot تکراری                                    |
| اشتباه‌گرفتن close-to-tray با خروج برنامه                  | تعریف عمر نشست بر اساس process، reset فقط در launch تازه و تست جدا برای hide/show و Quit/relaunch                    |
| جابه‌جایی sent/received یا جمع‌زدن چند منبع                | mapping صریح upload/download، یک منبع authoritative و workload ۱۰۰ MiB end-to-end                                    |
| بازشدن پنجره بدون رفتن به Dashboard                        | action مستقل، event typed قابل بازیابی، show/unminimize/focus و assertion روی page فعال                              |
| ظاهرشدن لحظه‌ای Paste و بازگشت مقدار قبلی React            | native setter + InputEvent، تست controlled component و assertion پس از render مجدد                                   |
| پاک‌شدن متن ورودی پس از failure ثبت رول                    | نتیجهٔ صریح success/failure از store و clear فقط بعد از persist/apply موفق                                           |
| تغییر target پس از clipboard read ناهمگام                  | capture و اعتبارسنجی target/selection، توقف امن روی unmount و تست race                                               |

## تعریف Done محصولی

این کار فقط زمانی تمام است که کاربر روی نسخهٔ بسته‌بندی‌شده بتواند در حالت
اتصال فعال یک زیر‌دامنه را اضافه کند، ظرف سقف زمانی تعریف‌شده root و همهٔ
زیر‌دامنه‌ها را در مسیر درست ببیند، هم‌زمانی آپدیتر هیچ پیام busy نسازد، یک
آپدیت امضاشدهٔ واقعی روی AppImage/NSIS تا relaunch کامل شود، و دامنه‌های موج
اول از provider نگه‌داری‌شدهٔ BiFlow به‌همراه زیر‌دامنه‌هایشان DIRECT باشند.
همچنین Direct/VPN pinهای ساخته‌شده پیش از app update یا cloud-rule refresh
باید پس از ارتقا و restart در Linux و Windows بدون حذف یا تغییر outbound باقی
مانده باشند. در همان نسخه، دانلود کنترل‌شدهٔ ۱۰۰ MiB باید بدون دوبرابرشماری در
received دیده شود، hide/show شمارنده را حفظ کند و Quit/relaunch آن را صفر کند.
گزینهٔ Dashboard در context menu نیز باید از هر صفحه و وضعیت اتصال، پنجره را
باز و مستقیماً Dashboard را فعال کند. Paste در input کنترل‌شده باید پس از
renderهای بعدی باقی بماند و متن رول فقط بعد از ثبت موفق پاک شود؛ هیچ failure
یا خطای clipboard نباید ورودی کاربر را از بین ببرد.
