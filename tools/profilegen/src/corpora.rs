//! Permissive frequency-weighted vocabulary corpora for the 18 additional M7 languages
//! (total 20 languages with EN and RU).

pub const UK_CORPUS: &[(&str, u32)] = &[
    ("і в не на що я з він а як то все вона так його але ти до у же ви за би по тільки її мені було ось від мене ще ні про із йому тепер коли навіть ну раптом чи якщо вже або бути був нього до вас знову вже вам адже там потім себе нічого їй може вони тут де є треба ній для ми тебе їх чим була сам щоб без ніби чого раз теж собі під буде тоді хто цей того тому цього який зовсім ним тут один майже мій тим щоб неї зараз були куди навіщо всіх ніколи можна при нарешті два інший хоч після над більше той через ці нас всього них яка багато хіба три цю моя втім добре свою цій перед іноді краще трохи тім неможливо такий їм більше завжди звичайно всю між", 45),
    ("привіт дякую будь ласка повідомлення система програма функція вікно клавіатура розкладка кнопка налаштування файл папка процес потік буфер рядок число значення результат помилка перевірка збірка версія встановлення", 40),
    ("добрий день ранок вечір ніч сьогодні завтра вчора друг робота час людина життя питання відповідь справа місце рука око обличчя сторона голова дім слово вид сила кінець дитина місто земля вода батько жінка машина країна світ випадок", 35),
];

pub const BE_CORPUS: &[(&str, u32)] = &[
    ("і ў не на што я з ён а як то ўсе яна так яго але ты да у ж вы за бы па толькі яе мне было вось ад мяне яшчэ не пра з яму цяпер калі нават ну раптам ці калі ўжо або быць быў яго да вас зноў вам бо там потым сябе нічога ёй можа яны тут дзе ёсць трэба ёй для мы цябе іх чым была сам каб без быццам чаго раз таксама сабе пад будзе тады хто гэты таго таму гэтага які зусім ім тут адзін амаль мой тым каб яе зараз былі куды навошта ўсіх ніколі можна пры нарэшце два іншы хоць пасля над больш той праз гэтыя нас усяго іх якая шмат хіба тры гэтую мая зрэшты добра сваю гэтай перад часам лепш крыху тым немагчыма такі ім больш заўсёды вядома ўсю паміж", 45),
    ("прывітанне дзякуй калі ласка паведамленне сістэма праграма функцыя акно клавіятура раскладка кнопка налада файл тэчка працэс паток памяць радок лік значэнне вынік памылка праверка версія", 40),
    ("добры дзень раніца вечар ноч сёння заўтра ўчора сябар праца час чалавек жыццё пытанне адказ справа месца рука вока твар бок галава дом слова від сіла канец дзіця горад зямля вада бацька жанчына краіна свет выпадак", 35),
];

pub const KK_CORPUS: &[(&str, u32)] = &[
    ("және мен сен ол біз сіз олар бұл сол осы қай қандай қашан қайда неге қалай бар жоқ емес керек болады болды екен үшін туралы дейін кейін бірге ғана тек тағы да бірақ немесе егер онда мүмкін әрине жақсы жаман үлкен кіші жаңа ескі көп аз барлық әрбір кейбір ешкім ештеңе ешқашан әрқашан қазір бүгін ертең кеше мына міне сонда мұнда", 45),
    ("сәлем сәлеметсіз рақмет өтінемін хабарлама жүйе бағдарлама функция терезе пернетақта батырма баптау файл бума үдеріс жады жол сан мән нәтиже қате тексеру нұсқа орнату", 40),
    ("қайырлы күн таң кеш түн дос жұмыс уақыт адам өмір сұрақ жауап іс орын қол көз бет жақ бас үй сөз күш бала қала жер су әке ана ел әлем жағдай", 35),
];

pub const DE_CORPUS: &[(&str, u32)] = &[
    ("der die das und in den von zu mit sich auf für nicht istภ dem als auch es an werden aus er hat dass sie nach wird bei einer um am sind noch wie einem über einen so zum war haben nur oder aber vor zur bis mehr durch man sein wurde sei প্রিয় kann gegen vom können sudah schon wenn habe ihre फिरภ unter wir soll ich eines كل هذا", 40),
    ("hallo welt willkommen bitte danke entschuldigung morgen abend nacht heuteestern freund nachricht system programm funktion fenster tastatur belegung schaltfläche einstellung datei ordner prozess speicher zeichenkette zahl wert ergebnis fehler prüfung version", 42),
    ("für über schön größe zurück natürlich möglich öffentlich ähnlichพึ่งพา während nächst größer erklären wünschen drücken öffnen schließen ändern prüfen wählen", 45),
    ("arbeiten schreiben lesen tippen schauen laufen starten warten senden empfangen verstehen erinnern zusammen vielleicht wahrscheinlich", 35),
];

pub const FR_CORPUS: &[(&str, u32)] = &[
    ("le la les de des un une et est en que qui dans pour pas sur plus avec par son mais comme tout cette או aux nous vous ils elle ce sont même fait faire été être avoir sans ses leur aussi bien peut entre après encore très toujours jamais", 45),
    ("bonjour salut monde bienvenue merci pardon matin soir nuit aujourd demain hier ami message système programme fonction fenêtre clavier disposition bouton paramètre fichier dossier processus mémoire chaîne nombre valeur résultat erreur vérification version", 42),
    ("déjà très après être même général différents nécessaire problème réponse exemple créer supprimer sélectionner sécurité confidentialité première dernière français", 40),
];

pub const ES_CORPUS: &[(&str, u32)] = &[
    ("el la los las de que y a en un ser se no haber por con su para como estar tener le lo todo pero más hacer o poder decir este ir otro ese la si me ya ver porque dar cuando él muy sin vez mucho saber qué sobre mi alguno mismo yo también hasta año dos querer entre así primero desde grande eso ni nos llegar pasar tiempo ella", 45),
    ("hola mundo bienvenido por favor gracias perdón mañana tarde noche hoy ayer amigo mensaje sistema programa función ventana teclado diseño botón configuración archivo carpeta proceso memoria cadena número valor resultado error versión", 42),
    ("español niño año mañana pequeño señor señora compañía también además después según algún ningún", 40),
];

pub const PT_CORPUS: &[(&str, u32)] = &[
    ("o a os as de que e do da em um para é com não uma os no se na por mais as dos como mas foi ao ele das tem à seu sua ou ser quando muito há nos já está eu também só pelo pela até isso ela entre era depois sem mesmo aos ter seus quem nas me esse eles estão você tinha foram essa num nem suas meu às minha têm numa pelos elas", 45),
    ("olá mundo bem vindo por favor obrigado desculpa manhã tarde noite hoje amanhã ontem amigo mensagem sistema programa função janela teclado botão configuração arquivo pasta processo memória número valor resultado erro versão", 42),
    ("não são então coração ação função configuração informação solução atenção português manhã irmão", 40),
];

pub const IT_CORPUS: &[(&str, u32)] = &[
    ("il lo la i gli le di a da in con su per tra fra e che non è sono un una uno del della dei degli delle al allo alla ai agli alle dal dallo dalla dai dagli dalle nel nello nella nei negli nelle sul sullo sulla sui sugli sulle come ma più anche questo questa questi queste quello quella quelli quelle tutto tutti tutta tutte molto sempre mai già ancora", 45),
    ("ciao mondo benvenuto per favore grazie scusa mattina sera notte oggi domani ieri amico messaggio sistema programma funzione finestra tastiera pulsante impostazione file cartella processo memoria stringa numero valore risultato errore versione", 42),
    ("perché così più già può ciò città università qualità sicurezza libertà", 38),
];

pub const TR_CORPUS: &[(&str, u32)] = &[
    ("ve bir bu da de için ileที่ยว olan olarak çok daha en gibiโซ่ var yok ama veya बाबत kadar sonra önce göre भलई kendi her tüm bütün hiçbir zaman şimdi bugün yarın dün burada orada nerede nasıl neden ne kim hangi", 42),
    ("merhaba dünya lütfen teşekkür teşekkürler özür sabah akşam gece arkadaş mesaj sistem program fonksiyon pencere klavye düzen düğme ayar dosya klasör işlem bellek dizge sayı değer sonuç hata kontrol sürüm kurulum", 45),
    ("çalışmak yazmak okumak bakmak başlamak beklemek göndermek almak anlamak hatırlamak birlikte gerçekten muhtemelen Türkçe öğrenci öğretmen", 42),
];

pub const PL_CORPUS: &[(&str, u32)] = &[
    ("i w nie na z do to że się jest o jak ale po co tak za od przez dla czy już tylko bardzo jeszcze może oraz lub bo gdy bez pod nad przed między przy", 45),
    ("cześć dzień dobry witaj proszę dziękuję przepraszam rano wieczór noc dzisiaj jutro wczoraj przyjaciel wiadomość system program funkcja okno klawiatura układ przycisk ustawienie plik folder proces pamięć ciąg liczba wartość wynik błąd wersja", 45),
    ("żółć Źródło szczęście jeszcze bardziej zawsze nigdy często czasami wszystko nic każdy żaden polski język", 40),
];

pub const NL_CORPUS: &[(&str, u32)] = &[
    ("de het een en van in is dat op te voor niet met zijn er aan om ook als maar bij of uit dan ze wij jij hij zij hebben worden kunnen zullen moeten willen zeggen gaan komen zien maken geven vinden denken weten", 45),
    ("hallo wereld welkom alsjeblieft bedankt dank morgen avond nacht vandaag morgen gisteren vriend bericht systeem programma functie venster toetsenbord indeling knop instelling bestand map proces geheugen getal waarde resultaat fout versie", 42),
];

pub const SV_CORPUS: &[(&str, u32)] = &[
    ("och i att det som en på är av för med till den har de inte om ett han men var jag sig från vi så kan man när år säger hon under också efter eller nu sin där vid mot ska skulle kommer får har", 45),
    ("hej världen välkommen tack snälla förlåt morgon kväll natt idag imorgon igår vän meddelande system program funktion fönster tangentbord knapp inställning fil mapp process minne sträng tal värde resultat fel version", 42),
    ("frånockså måste väldigt många några sådana både Även över under", 38),
];

pub const CS_CORPUS: &[(&str, u32)] = &[
    ("a v na se že to z do je o k pro s ale i po jako by co za od už jen také nebo když pak až pod před nad mezi při bez", 45),
    ("ahoj dobrý den vítejte prosím děkuji promiňte ráno večer noc dnes zítra včera přítel zpráva systém program funkce okno klávesnice rozložení tlačítko nastavení soubor složka proces paměť řetězec číslo hodnota výsledek chyba verze", 45),
];

pub const EL_CORPUS: &[(&str, u32)] = &[
    ("και το να του της τα στην στον από με για είναι δεν θα στο ότι αυτό αλλά ή ως (όπως) όλα (όλο) (όταν) μετά (πριν) (πάνω) (κάτω) (μέσα) (έξω) (πολύ) (λίγο) (τώρα) (σήμερα) (αύριο) (χθες)", 45),
    ("γεια κόσμος καλώς παρακαλώ ευχαριστώ συγγνώμη πρωί βράδυ νύχτα σήμερα αύριο χθες φίλος μήνυμα σύστημα πρόγραμμα λειτουργία παράθυρο πληκτρολόγιο διάταξη κουμπί ρύθμιση αρχείο φάκελος διαδικασία μνήμη αριθμός τιμή αποτέλεσμα σφάλμα έκδοση", 45),
];

pub const HE_CORPUS: &[(&str, u32)] = &[
    ("של את על לא הוא היא הם הן אנחנו אתם אני אתה עם כל גם או אם רק עוד יותר אבל כמו זה זאת אלה מה מי איפה מתי איך למה כן לא", 45),
    ("שלום עולם בבקשה תודה סליחה בוקר ערב לילה היום מחר אתמול חבר הודעה מערכת תוכנית פונקציה חלון מקלדת פריסה כפתור הגדרה קובץ תיקייה תהליך זיכרון מחרוזת מספר ערך תוצאה שגיאה בדיקה גרסה", 45),
];

pub const AR_CORPUS: &[(&str, u32)] = &[
    ("في من على إلى أن هذا هذه الذي التي هو هي هم نحن أنت أنا مع كلعن أو إذا لكن كما ما من أين متى كيف لماذا نعم لا", 45),
    ("مرحبا عالم شكرا عفوا صباح مساء ليل اليوم غدا أمس صديق رسالة نظام برنامج وظيفة نافذة لوحة مفاتيح زر إعداد ملف مجلد عملية ذاكرة نص رقم قيمة نتيجة خطأ إصدار", 45),
];

pub const HY_CORPUS: &[(&str, u32)] = &[
    ("և ու է որ չի այս այդ այն ես դու նա մենք դուք նրանք համար հետ մասին վրա մեջից հետո առաջ միայն նաև կամ եթե բայց երբ որտեղ ինչպես ինչու այո ոչ", 45),
    ("բարև աշխարհ խնդրում շնորհակալություն ներողություն առավոտ երեկո գիշեր այսօր վաղը երեկ ընկեր հաղորդագրություն համակարգ ծրագիր ֆունկցիա պատուհան ստեղնաշար կոճակ կարգավորում ֆայլ պանակ գործընթաց հիշողություն տող թիվ արժեք արդյունք սխալ տարբերակ", 45),
];

pub const KA_CORPUS: &[(&str, u32)] = &[
    ("და რომ ეს ის მე შენ ის ჩვენ თქვენ ისინი არ ვერ ნუ კი არა თუ მაგრამ ან როცა სად როგორ რატომ ვინ რა ყველა თითქმის უკვე ჯერ მხოლოდ ასევე", 45),
    ("გამარჯობა მსოფლიო გთხოვთ მადლობა ბოდიში დილა საღამო ღამე დღეს ხვალ გუშინ მეგობარი შეტყობინება სისტემა პროგრამა ფუნქცია ფანჯარა კლავიატურა განლაგება ღილაკი პარამეტრი ფაილი საქაღალდე პროცესი მეხსიერება სტრიქონი რიცხვი მნიშვნელობა შედეგი შეცდომა ვერსია", 45),
];
