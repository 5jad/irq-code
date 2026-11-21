# IRQ 0.1 — نطاق التنفيذ الحالي

**هذا الملف يلخص ما ينفذه compiler الآن، ولا يحل محل المواصفة المعيارية.** القرارات النهائية للغة ودلالاتها وأمثلة pass/fail موجودة في [`irq-0.1-language-spec.md`](irq-0.1-language-spec.md). الملف [`implemented-subset.ebnf`](implemented-subset.ebnf) يصف subset المنفذ حالياً. نجاح البرنامج في هذا subset لا يعني توافق compiler الكامل مع IRQ 0.1.

## ما ينفذ الآن

المشروع Rust workspace. CLI يوفر `check`, `emit-rust`, و`build` إلى rlib. الـlexer/parser يتعاملان مع ملف ووحدة واحدة، توقيعات الدوال والأقسام الإلزامية، scalar types `i32`, `i64`, `u32`, `u64`, `bool`, `str`, strings UTF-8 وبعض escapes، تعابير arithmetic/logical محدودة، استدعاءات محلية، `let`/`let mut`, إسناد، `return`, `if/else`, و`while`. بعد الفحص، `irq check` ينفذ lowering إلى crate `irq-hir` ذات typed HIR أولي؛ Rust emitter لا يزال يقرأ AST مباشرة.

الفاحص يتحقق من الأنواع المحلية، ثبات المتغيرات، تواقيع الدوال والاستدعاءات المحلية، شروط `if` و`while` من `bool`, تدفق الرجوع المدعوم، حدود literals/الثوابت والمقام الثابت الصفري. Rust backend يستخدم checked arithmetic ويحوّل overflow أو القسمة الديناميكية على صفر إلى `IrqRuntimeError`; `build` ينشر rlib بعد نجاح `rustc` فقط.

الاختبارات المرجعية الحالية في وحدات crates و`examples/`؛ كل مثال executable يجب أن يجتاز `cargo test --workspace` أو `irq check`. المثال `while_count.irq` يختبر loop syntax/backend، لكنه لا يثبت دعم باقي أنواع الحلقات أو loop invariants.

## غير منفذ بعد

لا يدعم compiler حالياً `()`، أنواع `Result`/error sets المصدرية، `ok/err/try/match`, تحويلات عددية، `+%`/`+?`, `struct`/`enum`/fields, `use` والوحدات المتعددة، collections، references/ownership checking، `for`/`break`, IO/capabilities enforcement، تحليل `reads/writes`, تنفيذ العقود، SMT، أو sandbox. HIR الأولي لا يضيف دلالة ملكية أو ضمان سلامة ذاكرة. الأقسام `requires` و`ensures` و`reads/writes` تُقرأ بنيوياً فقط؛ لا تثبت عقوداً ولا تفرض صلاحيات. لا يوجد ضمان memory safety أو عزل للبرامج الناتجة.

## أوامر التحقق

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p irq -- check examples/hello.irq
cargo run -p irq -- check examples/while_count.irq
cargo run -p irq -- build examples/while_count.irq /tmp/irq_while_count.rlib
```

حالة كل ميزة في [`TODO.md`](../TODO.md). تُحدّث هذه الوثيقة عند كل تغيير تنفيذي، مع الحفاظ على الفصل بين قرار اللغة وما ينفذه compiler فعلياً.
