# IRQ

لغة برمجة موجّهة للكود الذي تولّده نماذج الذكاء الاصطناعي، مع فحص صريح للأنواع والأخطاء والآثار الجانبية.

## القرار المعماري

- تنفيذ المترجم والأدوات بلغة Rust.
- تبقى دلالة IRQ مستقلة عن Rust؛ لا تعتبر IRQ مجرد صياغة بديلة له.
- في المرحلة الأولى، يمكن توليد Rust كـ backend مؤقت للاستفادة من النظام البيئي والتحسينات.
- تثبت واجهات AST وHIR وIR قبل إضافة backend أصلي مثل Cranelift أو LLVM.

## مراحل المترجم

`source -> lexer -> parser -> AST -> name/type checks -> typed HIR (initial) -> Rust backend -> rustc`

هذا هو مسار `irq check` الحالي، بما فيه lowering أولي إلى typed HIR. لا يزال Rust backend يولّد كود الـsubset مباشرة من AST؛ HIR الكامل وتحليل effects وتوليد backend منه مؤجلة.

العقود والتحقق الرمزي مرحلة لاحقة منفصلة. لا ندّعي إثباتها في MVP قبل تعريف المنطق المدعوم واختباره.

## وثائق التصميم والبحث

- [المواصفة المعيارية لـIRQ 0.1](docs/irq-0.1-language-spec.md)
- [TODO وخارطة المراحل](TODO.md)
- [TODO 2 — سجل النواقص المتبقية وبوابات الإكمال](TODO2.md)
- [ملخص نطاق التنفيذ الحالي](docs/irq-mvp-spec.md)
- [قواعد EBNF للـsubset المنفذ حالياً](docs/implemented-subset.ebnf)
- [بحث أخطاء الكود المولد ومخاطره الأمنية](docs/ai-code-risk-research.md)
- [عقد كتابة IRQ بواسطة نماذج AI](docs/ai-authoring-contract.md)
- [سياسة MSRV والتبعيات](docs/dependency-policy.md)
- [صيغة المشروع التجريبية وتحميل irq.toml](docs/project-format.md)
- [حالة HIR وحدود نموذج الملكية](docs/hir-and-ownership.md)
- [JSON Schema للتشخيصات](docs/diagnostic.schema.json)

## بناء المشروع

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p irq -- check examples/hello.irq
cargo run -p irq -- check examples/if_mixed_return.irq
cargo run -p irq -- check examples/integer_boundaries.irq
cargo run -p irq -- check examples/strings.irq
cargo run -p irq -- check examples/while_count.irq
cargo run -p irq -- fmt examples/hello.irq > /tmp/hello.formatted.irq
cargo run -p irq -- emit-rust examples/checked_arithmetic.irq > /tmp/checked_arithmetic.rs
rustc --crate-type lib /tmp/checked_arithmetic.rs -o /tmp/libchecked_arithmetic.rlib
cargo run -p irq -- emit-rust examples/strings.irq > /tmp/strings.rs
rustc --crate-type lib /tmp/strings.rs -o /tmp/libstrings.rlib
cargo run -p irq -- build examples/checked_arithmetic.irq /tmp/libchecked_arithmetic.rlib
```

## الحالة الحالية

- الحد الأدنى لإصدار Rust هو 1.88.0، مثبت في `rust-toolchain.toml`. إعداد GitHub Actions يستهدف Linux وWindows وmacOS؛ نجاح تشغيل runners البعيدة لم يُثبت بعد. تبعية runtime للكومبايلر/CLI صفر، و`serde_json` للاختبارات فقط.
- أخطاء lexer/parser/typecheck وI/O/backend التي يخرجها CLI تستخدم رموزاً ثابتة وغلاف JSON موثقاً. SourceMap يحتفظ بملفات متعددة، وإزاحات byte UTF-8 وأعمدة Unicode scalar؛ CLI حالياً يترجم ملفاً واحداً في الاستدعاء.
- اختبارات التكامل تغطي compile-pass وcompile-fail، واختبارات lexer/parser تشمل deterministic fuzz-smoke ومدخلات حجم/تعشيق كبيرة. هذه ليست بديلاً عن حملة cargo-fuzz طويلة المدة.

- يعمل lexer ويحوّل الرموز مع إزاحات byte-accurate لـUTF-8، وله اختبارات عربية للمواقع والأخطاء.
- يعمل parser أولي يتحقق من اسم الوحدة ورأس الدالة والأقسام، ويبني AST لـ`let`, الإسناد و`return` مع subset من التعبيرات.
- يدعم string literals UTF-8 من نوع `str` مع `\n`, `\t`, `\r`, `\"`, و`\\`.
- يدعم parser استدعاءات الدوال المحلية البسيطة، ويفحص وجودها وأنواع/عدد معاملاتها.
- يرفض سلاسل المقارنة المتتابعة وفق EBNF، ويدعم `irq fmt` كتطبيع محافظ للنهايات والمسافات الخارجية؛ لا يعيد formatter صياغة expressions.
- أخطاء parsing داخل تعبيرات الأسطر تعرض absolute UTF-8 byte spans؛ spans لكل AST node وأخطاء structural ذات token ranges ما زالت غير مكتملة.
- أضيفت حزمة `irq-project` لتحميل قائمة مصادر متعددة من `irq.toml` بصيغة manifest محدودة والتحقق من حدود المسارات وتكرار أسماء الوحدات؛ لم تُربط بعد بأمر CLI ولا تنفذ `use` أو حل الأسماء بين الوحدات.
- يدعم `if/else` المتداخل بشرط `bool` وتصريحات/إسنادات محلية بنطاق مستقل. يمكن للفرعين الرجوع أو الاستمرار، كما يمكن لمسار واحد أن يرجع والآخر يكمل بشرط أن يصل الأخير لاحقاً إلى `return`.
- يدعم `while` بشرط `bool` وكتلة متداخلة، ويعيد backend تقييم الشرط في كل دورة. الحلقة قد لا تعمل مطلقاً، فلا تُعدّ دليلاً على رجوع الدالة؛ `break` و`for` وتحليل invariants/control-flow الكامل غير متاحة.
- لا تزال `match` والحقول والمجموعات غير مدعومة في parser الحالي.
- يوجد فاحص أنواع أولي للأنواع scalar والمتغيرات والإسناد وقيمة `return`؛ ليس نظام أنواع كاملاً.
- يفحص المجال العددي للثوابت بحسب النوع المتوقع ويرفض القسمة أو باقي القسمة على صفر الثابتة؛ Rust backend يستخدم `checked_*` للحساب و`%` ويرجع `IrqRuntimeError` عند overflow أو القسمة/باقي القسمة الديناميكي على صفر.
- `irq check` يشغّل parse وفاحص الأنواع scalar وشرط/جسم `while` للـsubset ويرجع JSON يذكر ما فُحص وما لم يُفحص.
- `irq emit-rust` يرفض الملف إذا فشل parse/typecheck ثم يولّد Rust مستقلاً للـsubset.
- `irq build <file.irq> <output.rlib>` يشغّل `rustc` تلقائياً ويخرج مكتبة Rust للـsubset؛ لا ينشئ executable ولا يشغّل البرنامج. يترجم إلى ملف مؤقت بجانب الهدف ثم يستبدل الناتج بعد نجاح الترجمة فقط.
- **لا يثبت العقود ولا يفحص الآثار ولا يدعم كل IRQ أو يعزل البرنامج المولد.**
- المواصفة المعيارية لـIRQ 0.1 تفصل بوضوح القرارات المحسومة عن الميزات التي لم ينفذها compiler بعد.

مثال تشغيل:

```sh
cargo run -p irq -- check examples/hello.irq
```

# irq-code
