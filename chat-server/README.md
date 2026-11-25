# bytestrap-chat-server

Лёгкий WebSocket-релей для встроенного чата ByteStrap. Хранит все комнаты и участников строго в оперативной памяти без баз данных и истории сообщений.

## Запуск локально

```bash
cd chat-server
cargo run --release
```

По умолчанию сервер слушает `0.0.0.0:8787`. Изменить адрес/порт можно через переменную окружения `BIND_ADDR`:

```bash
BIND_ADDR=127.0.0.1:8787 cargo run --release
```

Проверка работоспособности:

```bash
curl http://127.0.0.1:8787/health
# ok
```

## Маршруты

- `GET /health` — возвращает `"ok"`
- `GET /ws` — WebSocket-подключение (комнаты идентифицируются 64-символьным SHA-256 хэшем)

## Деплой на VPS с Caddy (`wss://`)

Пример `Caddyfile`:

```caddy
chat.example.com {
    reverse_proxy 127.0.0.1:8787
}
```

Caddy автоматически выпустит TLS-сертификат Let's Encrypt и проксирует `wss://chat.example.com/ws` на локальный `127.0.0.1:8787`.

В конфиге ByteStrap (`%LOCALAPPDATA%\ByteStrap\config.json`) укажите:

```json
"chat_server_url": "wss://chat.example.com/ws"
```
