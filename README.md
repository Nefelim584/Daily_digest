# Daily Digest 🗞️

**Daily Digest** — это автономный, высокопроизводительный инструмент для сбора, фильтрации, дедупликации и суммаризации свежих новостей и научных статей из мира искусственного интеллекта и технологий.

Проект полностью переписан на **Rust** и собран в **единый автономный бинарный файл** без необходимости в установке внешних рантаймов (Python, Node.js) и без привязки к платным внешним API (Mistral, Gemini, OpenAI).

---

## Ключевые преимущества новой архитектуры (Pure Rust)

- ⚡ **Максимальная производительность**: мгновенный запуск (<10мс) и потребление памяти менее 30 МБ (по сравнению с 500+ МБ у Python-версии).
- 🔒 **100% локальная обработка (Zero API Keys)**:
  - **Суммаризация**: локальный алгоритм **TextRank** (граф предложений + PageRank), извлекающий 2–3 самых содержательных тезиса из каждого материала.
  - **Семантическая дедупликация и кластеризация**: локальная векторизация **TF-IDF**, подавление дубликатов по косинусному сходству и сферический **k-means**.
  - **Резервный веб-поиск (Fallback Scraper)**: встроенный легковесный HTML-парсер (`reqwest` + `scraper`), подхватывающий публикации при сбоях или блокировках RSS-лент и аккаунтов X (Twitter).
- 📊 **Интерактивная визуализация**: генерация дайджестов в форматах **Markdown (`.md`)**, **Plain Text (`.txt`)** и **HTML (`.html`)** с автоматически встроенной векторной SVG-диаграммой кластеров тем (проекция PCA/MDS).
- 🛡️ **Отказоустойчивость**: повторные запросы с экспоненциальным бэкоффом, учет заголовка `Retry-After` (HTTP 429), предохранитель (circuit breaker) и атомарная запись файлов (`.tmp` ➔ `rename`).
- 🌐 **Встроенный HTTP-демон (Axum)**: асинхронный REST API (`/health`, `/generate`, `/tasks/{id}`) с поддержкой очередей фоновых задач и вебхуков (`X-Secret-Token`).

---

## Архитектура пайплайна

```
[arXiv / Hugging Face / RSS / RSSHub / X]
                    │
                    ▼
  1. Сетевой сборщик (reqwest, backoff, HTTP 429 Retry-After, circuit breaker)
     └─► Резервный веб-скрейпер (при сбоях RSS/Twitter)
                    │
                    ▼
  2. Санитизация и очистка текста (удаление HTML, скриптов, стилей, обрезка)
                    │
                    ▼
  3. Фильтр временного окна (отсечение материалов старше N часов)
                    │
                    ▼
  4. Точная дедупликация (нормализация URL и заголовков)
                    │
                    ▼
  5. Классификатор важности (regex-правила: релизы, исследования, новости)
                    │
                    ▼
  6. Локальный TextRank-суммаризатор (извлечение ключевых предложений)
                    │
                    ▼
  7. TF-IDF векторизация, семантический дедуп и кластеризация тем (k-means + PCA)
                    │
                    ▼
  8. Сортировка по приоритетам и времени
                    │
                    ▼
  9. Форматирование (Markdown / Text / HTML с SVG-чартом) и атомарный экспорт
```

---

## Быстрый старт

### Требования
Для сборки из исходников необходим [Rust toolchain](https://rustup.rs/) (версии 1.80+):
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Сборка
```bash
# Клонирование репозитория
git clone https://github.com/your-username/Daily_Digest.git
cd Daily_Digest

# Сборка оптимизированного релизного бинарника
cargo build --release

# Исполняемый файл доступен по пути:
# ./target/release/daily-digest
```

### Статическая сборка (Static MUSL)
Для получения полностью автономного бинарного файла под Linux без внешних libc-зависимостей:
```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
# Бинарник: target/x86_64-unknown-linux-musl/release/daily-digest
```

---

## Использование CLI

```bash
# Базовый запуск (сбор новостей за последние 24 часа в формате Markdown):
./target/release/daily-digest

# Выборка за 72 часа (3 суток):
./target/release/daily-digest --hours 72

# Экспорт в интерактивный HTML с диаграммой тем:
./target/release/daily-digest --format html

# Экспорт в простой текстовый файл:
./target/release/daily-digest --format text

# Указание пользовательской папки сохранения:
./target/release/daily-digest --output /var/data/digests

# Отключение блока суммаризации (только очищенные сниппеты):
./target/release/daily-digest --no-summary

# Запуск с подробным выводом логов:
./target/release/daily-digest -v

# Запуск в режиме фонового HTTP-сервера:
./target/release/daily-digest --serve
```

---

## Конфигурация (`config/config.yaml`)

Все параметры настраиваются в файле [`config/config.yaml`](config/config.yaml):

```yaml
server:
  host: "0.0.0.0"
  port: 8000

time_window:
  default_hours: 24

output:
  format: "markdown"              # "markdown", "text", или "html"
  directory: "data/digests"       # Путь для сохранения дайджестов
  filename_pattern: "digest_{date}.{ext}"

cleaning:
  max_characters: 1500            # Длина сниппета на границе слов
  strip_html_tags: true
  strip_images: true
  strip_links: true
  strip_scripts: true

rsshub:
  base_url: "https://rsshub.app"
  timeout_seconds: 15

# Локальная экстрактивная суммаризация (TextRank)
summarization:
  enabled: true
  max_sentences: 3

# TF-IDF семантическая дедупликация и кластеризация
clustering:
  enabled: true
  dedup_threshold: 0.80           # Порог косинусного сходства для подавления дублей
  num_clusters: 0                 # 0 = автоматический подбор числа тем

# Встроенный fallback-скрейпер для сбойных лент
search:
  enabled: true
  pacing_seconds: 1.0

# Правила приоритизации по регулярным выражениям
priorities:
  - name: "Major Releases & Breakthroughs"
    priority: 100
    regex: "(?i)(release|introducing|launch|breakthrough|gpt-\\d|claude-\\d|gemini|llama|mistral|deepseek|sota)"
  - name: "Research & Benchmarks"
    priority: 50
    regex: "(?i)(paper|benchmark|dataset|architecture|evaluation|reasoning|alignment|vulnerability|jailbreak)"
  - name: "General News & Updates"
    priority: 10
    regex: ".*"

# Источники (arXiv, Hugging Face, RSS, RSSHub)
sources:
  - name: "arXiv AI & Security Papers"
    type: "arxiv_api"
    enabled: true
    categories: ["cs.AI", "cs.CL", "cs.LG", "cs.CV", "stat.ML", "cs.CR"]
    max_results: 50
  # ... блоги лабораторий и RSSHub маршруты
```

### 12-Factor переменные окружения
Настройки можно переопределять через переменные окружения или локальный `.env`:
- `DIGEST_OUTPUT_DIR` / `BOT_DIGESTS_PATH` — путь к папке дайджестов.
- `DIGEST_OUTPUT_FORMAT` — формат вывода (`markdown`, `text`, `html`).
- `DIGEST_DEFAULT_HOURS` — окно сбора новостей (в часах).
- `HOST` и `PORT` — сетевой интерфейс и порт HTTP-сервера.
- `RSSHUB_BASE_URL` — базовый адрес инстанса RSSHub.

---

## HTTP Сервер и интеграция с микросервисами 🐳

Для интеграции с Telegram-ботами (например, `Utility_bot`), панелями мониторинга или оркестраторами сервис предоставляет асинхронный HTTP API.

### Эндпоинты

#### 1. Проверка работоспособности
```http
GET /health
```
Ответ (`200 OK`):
```json
{
  "status": "ok",
  "version": "0.1.0"
}
```

#### 2. Запуск генерации дайджеста
```http
POST /generate
Content-Type: application/json

{
  "hours": 24,
  "format": "html",
  "webhook_url": "http://utility_bot:8080/callback",
  "webhook_secret_token": "my-secret-key"
}
```
Ответ (`202 Accepted`):
```json
{
  "task_id": "a50c822e-131d-4007-8898-d14cf1b8dfd5",
  "status": "pending"
}
```

#### 3. Статус задачи
```http
GET /tasks/{task_id}
```
Ответ (`200 OK`):
```json
{
  "task_id": "a50c822e-131d-4007-8898-d14cf1b8dfd5",
  "status": "completed",
  "created_at": "2026-10-16T12:00:00Z",
  "completed_at": "2026-10-16T12:00:02Z",
  "output_path": "data/digests/digest_2026-10-16_1200.html",
  "error": null
}
```

#### 4. Вебхук-оповещение
После завершения генерации сервис автоматически отправляет `POST` на указанный `webhook_url` с заголовком `X-Secret-Token: my-secret-key` и телом:
```json
{
  "task_id": "a50c822e-131d-4007-8898-d14cf1b8dfd5",
  "status": "completed",
  "output_path": "data/digests/digest_2026-10-16_1200.html",
  "error": null
}
```

---

## Docker и Docker Compose

Сервис упакован в многоэтапный образ на базе Debian slim с минимальным размером и строгой изоляцией прав (`appuser`, UID/GID 1000).

```bash
# 1. Сборка образа
docker compose -f docker/docker-compose.yml build

# 2. Однократный запуск генерации:
docker compose -f docker/docker-compose.yml run --rm daily-digest

# 3. Фоновый планировщик Ofelia (автозапуск по расписанию):
docker compose -f docker/docker-compose.yml up -d scheduler
```

---

## Тестирование

Кодовая база полностью покрыта встроенными тестами Rust (unit-тесты модулей, парсинг конфигурации, очистка HTML, TextRank, TF-IDF, фильтры и интеграционные тесты HTTP-сервера Axum):

```bash
# Запуск всех тестов:
cargo test

# Проверка линтером Clippy:
cargo clippy -- -D warnings
```
