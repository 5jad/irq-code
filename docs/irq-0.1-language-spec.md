# المواصفة المعيارية للغة IRQ 0.1

**الحالة:** قرارات التصميم لـIRQ 0.1 مُثبتة في 2026-11-18. هذه الوثيقة هي المرجع المعياري للنطاق والدلالات. ليست ادعاءً بأن المترجم الحالي ينفّذ كل ما تصفه؛ حالة التنفيذ الدقيقة في §2 و`docs/implemented-subset.ebnf`.

**المبدأ:** دلالة IRQ مستقلة عن Rust. Rust هو backend مؤقت فقط، ونجاح `rustc` لا يثبت سلامة برنامج IRQ أو صحة عقده أو عزله.

## 1. نطاق الإصدار

IRQ 0.1 هو profile صغير أحادي الوحدة، مصمم لتوليد برامج ذات أنواع صريحة وفحص حتمي. الأنواع الأساسية المعيارية: `i32`, `u32`, `i64`, `u64`, `bool`, `str`, و`()` (unit). لا يوجد `null`.

يدخل profile النحوي المستهدف: دوال محلية، `pub`, متغيرات مهيأة صراحةً، الإسناد للمحليات القابلة للتغيير، تعابير صحيحة/منطقية/نصية، استدعاءات محلية، `if/else`, `while`, و`return`. يدعم نموذج الأخطاء المعياري `Result`, و`ok`, و`err`, و`try`, مع فحص شامل لـ`match` على النتيجة. هذه القواعد تُنفّذ تدريجياً؛ لا يعني إدراجها هنا أن parser الحالي يقبلها كلها.

لا تدخل في 0.1: `use` والوحدات متعددة الملفات، `const`, `struct`, `enum` المعرفة من المستخدم، الحقول، generics/traits، المراجع، `list`/`map`/`set`, `for`, `break`, floats، FFI، IO، التزامن، أو SMT. الكلمات `use`, `const`, `struct`, `enum`, `errdef`, `test`, `for`, `break`, `old`, `null`, و`opt` محجوزة للتوسع. `match`, `try`, `ok`, `err`, و`Result` جزء من صياغة 0.1 المستهدفة لكنها غير منفذة في compiler الحالي.

## 2. الفصل بين المعياري والمنفذ

| المجال | قرار IRQ 0.1 | حالة المترجم الحالي |
| --- | --- | --- |
| الملف والوحدة | وحدة واحدة، دوال مرتبة، أقسام توقيع ثابتة | مدعوم جزئياً |
| الأنواع | الأنواع الأساسية أعلاه، بما فيها `()` | scalar `i32/i64/u32/u64/bool/str` و`()` مدعومة في subset الحالي |
| التحكم | `if/else`, `while`; لا `for` ولا `break` | `if/else` و`while` subset مدعومان |
| الحساب | checked semantics المبينة في §6 | checked add/sub/mul/div/neg في Rust backend؛ الفاحص يرفض بعض الثوابت غير الصالحة |
| الأخطاء | `Result<T,E>` صريح؛ لا إسقاط للخطأ | backend يحول أخطاء الحساب إلى `Result` داخلي، لكن لا يوجد `try`/`ok`/`err`/`match` مصدري حتى الآن |
| العقود | annotations غير فعّالة في 0.1، لا إثبات ولا runtime checking | الأقسام مطلوبة نصياً ولا تُفسر دلالياً |
| الآثار | profile 0.1 بلا IO؛ `reads/writes: none` فقط معيارياً | الأقسام مطلوبة، لكن محتواها لا يخضع لتحليل أثر |
| الملكية | قيم مملوكة؛ لا مراجع أو aliasing مصدري | subset لا يدعم نظام ملكية عاماً؛ لا يُدّعى memory safety |

أي اختلاف بين هذه المواصفة والبرنامج الحالي هو **فجوة تنفيذ**، وليس استثناءً ضمنياً من الدلالة. يبقى `docs/implemented-subset.ebnf` وصفاً لما ينجح فعلياً في الـsubset الحالي، ويُحدّث مع كل تنفيذ.

## 3. الأسماء والرموز والتنسيق

- المصدر UTF-8، لكن identifiers في 0.1 ASCII فقط: `[A-Za-z_][A-Za-z0-9_]*`، حساسة لحالة الأحرف. إعادة تعريف اسم في النطاق نفسه خطأ.
- الكلمات المحجوزة في profile: `mod`, `pub`, `fn`, `reads`, `writes`, `requires`, `ensures`, `body`, `none`, `let`, `mut`, `if`, `else`, `while`, `return`, `or`, `and`, `not`, `true`, `false`, وأنواع `i32`, `u32`, `i64`, `u64`, `bool`, `str`, `Result`؛ و`use`, `const`, `struct`, `enum`, `errdef`, `test`, `for`, `break`, `old`, `null`, `opt` محجوزة للتوسع. `match`, `try`, `ok`, و`err` كلمات محجوزة ومحددة في grammar المعيارية.
- التعليقات في 0.1 أسطر مستقلة تبدأ بـ`#` بعد صفر أو أكثر من المسافات وتمتد لنهاية السطر؛ التعليق اللاحق للكود غير قانوني. الأسطر الفارغة مسموحة بين التعريفات، لا داخل جملة متعددة الأسطر إلا حيث تحددها grammar. newline القياسي LF؛ تقبل أدوات المصدر CRLF بعد التطبيع.
- لا تستخدم tabs في indentation. رأس الدالة وأقسام التوقيع بلا indentation؛ داخل الدالة سطر `reads/writes/requires/ensures/body` بمسافتين؛ تعليمات `body` بأربع مسافات، وكل مستوى كتلة أعمق بمسافتين. لا توجد تعليمات على المستوى الأعلى خارج الدوال.
- parser يرفض indentation غير القانوني، الكتل الفارغة، الأقسام المكررة/الناقصة/المقلوبة، والكلمات المحجوزة كأسماء. لا يغيّر `fmt` مستقبلي المعنى.

## 4. EBNF المعيارية لـIRQ 0.1

هذه grammar للـprofile المعياري المستهدف، وليست وصفاً كاملاً لما يدعمه parser حالياً. `indent(n)` تعني بالضبط `n` مسافة ASCII، و`NL` نهاية سطر واحدة. التعليقات تُحذف مع الحفاظ على مواقع المصدر قبل التحليل. المسافات خارج indentation اختيارية حول العوامل؛ `SP` في grammar يفرض فصلاً بمسافة واحدة أو أكثر حيث ورد صراحةً.

```ebnf
source          = { blank | comment_line }, module_decl, NL,
                  { blank | comment_line }, function_decl,
                  { blank | comment_line | function_decl }, EOF ;
module_decl     = "mod", SP, identifier ;
function_decl   = [ "pub", SP ], "fn", SP, identifier, "(", [ parameters ], ")",
                  SP, "->", SP, return_type, NL,
                  indent(2), "reads:", SP, effect_list, NL,
                  indent(2), "writes:", SP, effect_list, NL,
                  indent(2), "requires:", SP, contract_expression, NL,
                  indent(2), "ensures:", SP, contract_expression, NL,
                  indent(2), "body:", NL, block(4), NL ;
parameters      = parameter, { ",", SP, parameter } ;
parameter       = identifier, ":", SP, type ;
return_type     = value_type | value_type, SP, "|", SP, "err(", error_name,
                  { ",", SP, error_name }, ")" ;
effect_list     = "none" ;
contract_expression = expression ;
type            = value_type | result_type ;
value_type      = "i32" | "u32" | "i64" | "u64" | "bool" | "str" | "()" ;
result_type     = "Result<", value_type, ",", SP, error_set, ">" ;
error_set       = error_name, { SP, "|", SP, error_name } ;
block(n)        = { blank | comment_line }, indent(n), statement(n),
                  { NL, { blank | comment_line }, indent(n), statement(n) } ;
statement(n)    = let_stmt | assignment | return_stmt | if_stmt(n)
                | while_stmt(n) | result_match(n) ;
let_stmt        = "let", [ SP, "mut" ], SP, identifier, ":", SP, type,
                  SP, "=", SP, expression ;
assignment      = identifier, SP, "=", SP, expression ;
return_stmt     = "return", SP, expression ;
if_stmt(n)      = "if", SP, expression, ":", NL, block(n+2), NL,
                  indent(n), "else:", NL, block(n+2) ;
while_stmt(n)   = "while", SP, expression, ":", NL, block(n+2) ;
result_match(n) = "match", SP, expression, ":", NL,
                  indent(n+2), "ok(", identifier, "):", NL, block(n+4), NL,
                  indent(n+2), "err(", identifier, "):", NL, block(n+4) ;
expression      = logical_or ;
logical_or      = logical_and, { SP, "or", SP, logical_and } ;
logical_and     = comparison, { SP, "and", SP, comparison } ;
comparison      = sum, [ comparison_op, sum ] ;
comparison_op   = "==" | "!=" | "<" | "<=" | ">" | ">=" ;
sum             = product, { ( "+" | "-" ), product } ;
product         = unary, { ( "*" | "/" | "%" ), unary } ;
unary           = ( "not" | "-" | "try" ), unary | call | primary ;
call            = identifier, "(", [ arguments ], ")" ;
arguments       = expression, { ",", SP, expression } ;
primary         = integer | string | "true" | "false" | identifier
                | "ok(", expression, ")" | "err(", error_name, ")"
                | "(", expression, ")" ;
identifier      = ( ASCII_ALPHA | "_" ), { ASCII_ALNUM | "_" } ;
integer         = DIGIT, { DIGIT } ;
string          = '"', { UTF8_CHAR_EXCEPT_QUOTE_BACKSLASH_NEWLINE | escape }, '"' ;
escape          = "\\n" | "\\t" | "\\r" | "\\\"" | "\\\\" ;
error_name      = "Overflow" | "DivisionByZero" | "ContractViolation" ;
comment_line    = { SP }, "#", { any_non_newline_character }, NL ;
blank           = { SP }, NL ;
SP              = " ", { " " } ;
indent(n)       = n * " " ;
ASCII_ALPHA     = ? one ASCII letter from A-Z or a-z ? ;
ASCII_ALNUM     = ASCII_ALPHA | DIGIT ;
DIGIT           = ? one ASCII digit from 0-9 ? ;
NL              = LF | CRLF ;
LF              = ? line feed U+000A ? ;
CRLF            = ? carriage return U+000D followed by line feed U+000A ? ;
EOF             = end_of_source ;
```

قيود دلالية إضافية: parameters فريدة؛ لا توجد إعادة تعريف محلية في النطاق نفسه؛ كل دالة تنتهي صراحةً بـ`return` على كل مسار (بما فيه unit عبر `return ()`)، و`if` لها فرعان؛ جسم `while` قد لا ينفذ إطلاقاً فلا يثبت الرجوع؛ `match` على نتيجة يجب أن يغطي `ok` و`err` بالضبط؛ المقارنة غير متسلسلة؛ call محلي فقط. `let` يتطلب annotation صريحاً. لا توجد implicit casts. النوع المحلي لنتيجة fallible هو `Result<T,E>`، و`-> T | err(...)` صياغة مختصرة له.

مثال نحوي معياري مقبول:

```irq
mod demo

pub fn answer() -> i32
  reads: none
  writes: none
  requires: true
  ensures: result == 42
  body:
    return 42
```

أمثلة مرفوضة معيارياً: دالة بلا `writes:`؛ تكرار `reads:`؛ `fn return() -> i32` لأن `return` محجوز؛ `mod café` لأن identifiers ASCII؛ indentation بـtab؛ `if x:` بلا `else:`؛ comparison chain مثل `a < b < c`؛ أو تعليمة على مستوى الوحدة خارج دالة. يميز compiler الحالي بعض هذه الحالات فقط؛ اختبار التوافق الكامل باقٍ ضمن مرحلة parser.

## 5. الأنواع والقيم والرؤية

- `i32`, `u32`, `i64`, `u64` أعداد صحيحة ثابتة العرض؛ `bool` قيمتاه `true/false`؛ `str` نص UTF-8 غير قابل للتعديل بالقيمة؛ `()` قيمة الوحدة.
- الـscalars و`()` قيم مباشرة. `str` يتبع value semantics: الإسناد/التمرير لا يكشف alias قابل للتعديل. لا يعرّف 0.1 references أو lifetimes أو `clone` كمفهوم مصدر.
- لا يوجد `null`. غياب قيمة مستقبلياً يمثّل بنوع اختياري صريح، وليس null؛ `opt<T>` مؤجل عن 0.1.
- `pub fn` تصدّر الدالة من الوحدة، و`fn` وحدها خاصة. لا توجد وحدات أو imports متعددة الملفات في 0.1، لذلك لا توجد قواعد خصوصية للحقول.
- `const`, `struct`, `enum`, field access, `list<T>`, `map<K,V>`, و`set<T>` خارج النطاق. لا نعِد بسلامة الذاكرة لمجرد أن backend Rust.

**قبول:** `let mut n: i32 = 0` ثم `n = n + 1` بعد ضبط نموذج الخطأ؛ `let name: str = "IRQ"`; دالة `pub fn` ظاهرة على حد الوحدة.

**رفض:** `let n: i32 = 0` ثم `n = 1`; `let x: &i32 = &n`; `Point { x: 1 }`; `null`; `set<str>`؛ `use other::f` في 0.1.

## 6. الحساب الصحيح

- المجالات هي المجالات الرياضية المعتادة لأنواع 32/64-bit الموقعة وغير الموقعة. الثوابت الكاملة التي تتجاوز المجال المتوقع تُرفض وقت الفحص؛ لا يوجد narrowing ضمني.
- integer literal يتكون من أرقام عشرية بلا separators، ومقداره لا يتجاوز `u64::MAX`؛ يحدد السياق نوعه، لذلك `-9223372036854775808` صالح كـ`i64::MIN` بينما القيمة الموجبة نفسها لا تلائم `i64`.
- العمليات العادية `+`, `-`, `*`, unary `-`, `/`, `%` checked. إذا أمكن إثبات فشل العملية في الثابت، يرفضها الفاحص؛ وإذا كان الفشل ديناميكياً، ينتج `Result` بخطأ `Overflow` أو `DivisionByZero`. لا التفاف صامت ولا panic/UB كدلالة لغوية.
- العملية الثابتة التي يقيّمها compiler بأمان إلى قيمة داخل المجال تصبح قيمة scalar عادية؛ أما العملية الديناميكية القابلة للفشل فنوعها `Result<T,E>`، ويجب استعمال `try` أو `match` قبل استخدامها كـ`T`.
- `/` يقرب الناتج نحو الصفر. `a % b` هو الباقي الموافق لهذا quotient، أي `a = (a / b) * b + (a % b)`، وإشارة الباقي إشارة `a` أو صفر.
- القسمة والباقي على صفر ينتجان `DivisionByZero`. في signed types، `MIN / -1` و`MIN % -1` ينتجان `Overflow`.
- الأنواع العشرية وNaN/Infinity ليست في 0.1، ولا توجد أي دلالة عائمة ضمنية.
- `+%` و`+?` لا يقبلان في 0.1 ولا تُنسب إليهما دلالة مؤقتة. إذا أضيفا، يلزم قرار مستقل واختبارات؛ لا يجوز استنتاج معنى من الاسم.
- التحويلات العددية الصريحة مؤجلة؛ في 0.1 لا casts. المزج بين نوعين في العملية arithmetic خطأ.

**قبول:** `7 / 3 = 2`; `-7 / 3 = -2`; `-7 % 3 = -1`; قيمة `i32` ثابتة داخل المجال.

**رفض:** `i32::MAX + 1` كقيمة ثابتة في سياق `i32`; قسمة ثابتة على `(3 - 3)`; جمع `i32` مع `i64` من دون تحويل؛ `i32::MIN / -1`; `+%` أو `+?`؛ literal مثل `1.5` أو قيمة `NaN`/`Infinity` في profile 0.1.

## 7. الأخطاء ونتائج الدوال

التمثيل المعياري هو `Result<T, E>` (sum type مغلق في كل توقيع)، لا exceptions مخفية ولا sentinels ولا null. الكتابة المختصرة `-> T | err(E1, E2)` تعني أن الدالة ترجع إما `ok(T)` أو أحد الأخطاء المعلنة. أخطاء 0.1 المعيارية: `Overflow`, `DivisionByZero`, و`ContractViolation`؛ يمكن توسيع errors المعرفة من المستخدم لاحقاً بقرار منفصل.

- الدالة التي تعلن أخطاءً يجب أن تعيد `ok(value)` أو `err(error)`, أو تستخدم `try` لنشر error معلن من callee/operation. لا تسقط النتيجة ولا يوجد تجاهل ضمني.
- `try e` لا يقبل إلا `Result<T,E>`؛ عند `ok(v)` ينتج `v`، وعند `err(e)` يعيد الخطأ من الدالة الحالية، ويجب أن يكون الخطأ ضمن مجموعة أخطائها المعلنة.
- لمعالجة النتيجة محلياً، `match r:` يتطلب فرعي `ok(value)` و`err(error)`؛ كلا الفرعين إلزاميان. `match` ليس مطلوباً إذا كان الغرض propagation بـ`try`.
- الاستدعاء/العملية القابلة للفشل لا يمكن استعمالها كقيمة `T` دون `try` أو `match`. لا يسمح بترك `Result` غير مستهلك.
- `errdef` وأخطاء المستخدم مؤجلة؛ في 0.1 تستخدم tags المعرفة أعلاه فقط.

**قبول معياري (غير مدعوم بعد في compiler):** `fn div(a: i32, b: i32) -> i32 | err(DivisionByZero, Overflow)`؛ ثم `match div(a,b):` مع فرعي `ok(q)` و`err(e)`.

مثال كامل معياري لمعالجة النتيجة (غير قابل للتشغيل في compiler الحالي):

```irq
fn add_checked(a: i32, b: i32) -> i32 | err(Overflow)
  reads: none
  writes: none
  requires: true
  ensures: true
  body:
    return ok(try (a + b))

fn use_add() -> i32
  reads: none
  writes: none
  requires: true
  ensures: true
  body:
    match add_checked(1, 2):
      ok(value):
        return value
      err(problem):
        return 0
```

**رفض معياري:** `let q: i32 = div(a,b)` بلا handling؛ `try div(a,b)` داخل دالة لا تعلن الخطأ؛ `return null`; إسقاط `Result` من دون `match` أو `try`.

## 8. العقود

- أقسام `requires` و`ensures` جزء من توقيع الدالة في صيغة source الحالية، لكنها **annotations غير فعّالة في IRQ 0.1**: parser قد يتحقق من وجودها/ترتيبها، لكن `check` و`build` لا يثبتانها ولا ينفذانها. لا يجوز استخدامها كقيد أمني أو ادعاء تحقق.
- لا يوجد SMT في 0.1، ولا معنى لـ`proven` أو ضمان ساكن للعقد. `old(x)` محجوز ومؤجل، وليس تعبيراً مدعوماً.
- القرار المستقبلي المحدود: إذا نُفذ `runtime(...)` لاحقاً، فإن `requires` تفحص عند الدخول و`ensures` عند الرجوع الناجح فقط، لا عند مسار الخطأ. يلزم حينها تعريف result/snapshot ونوع فشل العقد وحدود الموارد قبل التنفيذ.
- لا يسمح بتقييم عقد عبر arithmetic قد يلتف أو يسبب undefined behavior. عند بناء evaluator لاحقاً إما يستخدم حساباً رياضياً دقيقاً أو يرفض تعبيراً غير مدعوم؛ لا يُعتمد evaluator arithmetic عادي.

**قبول في 0.1:** وجود `requires: true` و`ensures: result == 3` في موضعهما النحوي مع فهم أن compiler الحالي لا يفحصهما.

**رفض:** الاستنتاج أن `ensures: result > 0` مثبت أو runtime-enforced؛ استخدام `old(x)` كأنه مدعوم؛ إسناد ضمان أمني إلى contract annotation وحدها.

## 9. الآثار والقدرات

- IRQ 0.1 core pure: لا filesystem/network/process/environment/time/random IO من البرنامج. معيارياً قيمة كل من `reads:` و`writes:` هي `none` فقط.
- النص `reads/writes` وحده ليس capability ولا sandbox. compiler الحالي لا يحلل هذه الحقول دلالياً؛ لا تنسب إليه منع أثر جانبي.
- قرار معماري للإصدار الذي يضيف IO: capability صريحة محدودة يمررها host للبرنامج، مع فحص آثار ساكن عابر للدوال للكشف عن التصريحات غير المتسقة، وفرض فعلي عند runtime/OS. لا توجد ambient capabilities. يجب أن تتضمن اختبارات الإنفاذ traversal/symlink/TOCTOU وحدود الموارد قبل ادعاء العزل.
- `reads/writes` المستقبلية ملخص أثر ساكن وليست منحة صلاحية. المنح الفعلي يكون handles غير قابلة للتزوير يمررها host صراحةً؛ لا يستدعي البرنامج API للملفات/الشبكة دون handle. يربط runtime كل handle بجذر/خدمة محددة ويستخدم إنفاذاً عند OS (مثل pre-opened descriptors أو عميل شبكة محدود) بحيث لا يكفي تطبيع string path وحده كحد أمني. تفاصيل API وصيغة أسماء القدرات تُعتمد في مرحلة 7 مع اختبارات عدائية.
- المكتبات الخارجية تبقى حدود ثقة منفصلة ولا يكفي تصريح أثرها لإثبات سلوكها.

**قبول في core 0.1:** `reads: none` و`writes: none`.

**رفض في core 0.1:** `reads: fs("/tmp")` أو أي ادعاء أن `reads: none` وحدها تمنع الوصول إلى الملفات.

## 10. أمثلة pass/fail وحدود التوافق

الأمثلة السابقة هي normative examples للغة 0.1. أمثلة parser/typechecker المنفذة حالياً في `examples/` واختبارات الوحدات تمثل executable subset أصغر. قبل تعليم أي syntax بـ`[x]` في خارطة التنفيذ، يجب أن توجد اختبارات compile-pass وcompile-fail تغطيه. لا تخلط بين اختبار grammar النظري واختبار المترجم.

الفجوات المعروفة التي يجب معالجتها لاحقاً تشمل: Result syntax وhandling، match، reserved-word enforcement الشامل، grammar-generated/conformance tests، دقة التعليقات داخل string، source spans لكل عقدة، contracts/effects checks، وقراءة الأقسام/التنسيق وفق القواعد أعلاه. دعم `()` منفذ في subset واختباره، ولا يعني اكتمال بقية grammar المعيارية.

## 11. حواجز الأمان والادعاءات

لا يضمن نجاح parse/typecheck صحة منطق البرنامج. لا يثبت Rust backend أن المصدر آمن، ولا تعني checked arithmetic وجود عزل. لا FFI أو كود Rust خام أو capabilities نظامية في core 0.1. لا توصف IRQ بأنها memory-safe أو sandboxed حتى توجد فحوص/إنفاذ واختبارات مستقلة تثبت ذلك.
