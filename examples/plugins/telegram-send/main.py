import json
import mimetypes
import os
import sys
import urllib.error
import urllib.request
import uuid

API = "https://api.telegram.org/bot{token}/{method}"
DOCUMENT_LIMIT = 50 * 1024 * 1024
PHOTO_LIMIT = 10 * 1024 * 1024
PHOTO_TYPES = {".jpg", ".jpeg", ".png", ".webp"}
TIMEOUT = 300
UNAUTHORIZED = 401


class Failure(Exception):
    pass


class Traydex:
    def __init__(self, request):
        self.request = request

    @staticmethod
    def send(kind, **fields):
        print(json.dumps({"type": kind, **fields}, ensure_ascii=False), flush=True)


def form(fields, field, path):
    boundary = uuid.uuid4().hex
    parts = [
        f'--{boundary}\r\nContent-Disposition: form-data; name="{name}"\r\n\r\n{value}\r\n'.encode()
        for name, value in fields.items()
    ]
    filename = os.path.basename(path).replace('"', "'")
    kind = mimetypes.guess_type(path)[0] or "application/octet-stream"
    head = (
        f'--{boundary}\r\nContent-Disposition: form-data; name="{field}"; filename="{filename}"\r\n'
        f"Content-Type: {kind}\r\n\r\n"
    )
    with open(path, "rb") as file:
        parts.append(head.encode() + file.read() + b"\r\n")
    parts.append(f"--{boundary}--\r\n".encode())
    return boundary, b"".join(parts)


def call(app, token, method, fields, field, path):
    boundary, body = form(fields, field, path)
    query = urllib.request.Request(
        API.format(token=token, method=method),
        data=body,
        headers={"Content-Type": f"multipart/form-data; boundary={boundary}"},
    )
    try:
        with urllib.request.urlopen(query, timeout=TIMEOUT) as response:
            answer = json.load(response)
    except urllib.error.HTTPError as error:
        try:
            answer = json.load(error)
        except ValueError:
            raise Failure(f"Telegram: HTTP {error.code}") from error
    except (urllib.error.URLError, TimeoutError) as error:
        reason = getattr(error, "reason", error)
        raise Failure("Can't reach Telegram: {}".format(reason)) from error
    if answer.get("ok"):
        return answer["result"]
    if answer.get("error_code") == UNAUTHORIZED:
        raise Failure("Telegram rejected the bot token")
    raise Failure("Telegram: " + answer.get("description", "request rejected"))


def link(message):
    username = message.get("chat", {}).get("username")
    return f"https://t.me/{username}/{message['message_id']}" if username else None


def deliver(app, path, index):
    settings = app.request["settings"]
    name = os.path.basename(path)
    size = os.path.getsize(path)
    if size > DOCUMENT_LIMIT:
        raise Failure("{} is over 50 MB, Telegram bots can't send it".format(name))
    photo = not settings.get("documents") and os.path.splitext(name)[1].lower() in PHOTO_TYPES and size <= PHOTO_LIMIT
    method, field = ("sendPhoto", "photo") if photo else ("sendDocument", "document")
    fields = {"chat_id": settings["chat"].strip()}
    caption = settings.get("caption", "").strip()
    if caption and index == 0:
        fields["caption"] = caption
    return call(app, settings["token"].strip(), method, fields, field, path)


def main(app):
    settings = app.request["settings"]
    if not settings.get("token", "").strip() or not settings.get("chat", "").strip():
        raise Failure("Set the bot token and the chat in the tile settings")
    files = app.request["files"]
    links = []
    for index, path in enumerate(files):
        try:
            message = deliver(app, path, index)
        except Failure as failure:
            if index == 0:
                raise
            raise Failure("Sent {} of {}. {}".format(index, len(files), failure))
        url = link(message)
        if url:
            links.append(url)
        app.send("progress", value=(index + 1) / len(files))
    if links:
        app.send("clipboard", text="\n".join(links))
    done = "Sent to Telegram: {}".format(len(files))
    if links:
        done += " · links copied"
    app.send("notify", text=done)


if __name__ == "__main__":
    traydex = Traydex(json.load(sys.stdin))
    try:
        main(traydex)
    except (Failure, OSError) as problem:
        traydex.send("error", text=str(problem))
        sys.exit(1)
