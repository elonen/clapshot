"""
Localization runtime for the basic_folders organizer.

gettext-based, sharing the central catalog (see i18n/README.md). The message id IS the English
source string; `msgctxt` disambiguates. Mark strings with `_(...)` / `pgettext(ctx, ...)`; they are
extracted by `xgettext --language=Python` and compiled to locale/<lang>/LC_MESSAGES/clapshot.mo.

The active locale is per-request (the user's UI language, forwarded by the server in
UserSessionData.language). It is held in a contextvar so each async request task translates
independently. An unset/unknown/`en` locale falls back to the source string.
"""
import gettext
import contextvars
from pathlib import Path
from functools import wraps
from typing import Optional, Callable, Awaitable, TypeVar, Any

_DOMAIN = "clapshot"
_LOCALE_DIR = Path(__file__).resolve().parent.parent / "locale"

_NULL = gettext.NullTranslations()
_cache: dict[str, gettext.NullTranslations] = {}
_current: contextvars.ContextVar[gettext.NullTranslations] = \
    contextvars.ContextVar("clapshot_i18n", default=_NULL)


def _translations_for(lang: Optional[str]) -> gettext.NullTranslations:
    if not lang or lang == "en":
        return _NULL
    if lang not in _cache:
        try:
            _cache[lang] = gettext.translation(_DOMAIN, localedir=str(_LOCALE_DIR), languages=[lang])
        except FileNotFoundError:
            _cache[lang] = _NULL  # no catalog for this locale -> source strings
    return _cache[lang]


def set_locale(lang: Optional[str]) -> None:
    """Set the active locale for the current request/task (from UserSessionData.language)."""
    _current.set(_translations_for(lang))


def _(message: str) -> str:
    """Translate `message` (the English source string) to the current locale."""
    return _current.get().gettext(message)


def pgettext(context: str, message: str) -> str:
    """Translate `message` within `context` (gettext msgctxt) to the current locale."""
    return _current.get().pgettext(context, message)


def ngettext(singular: str, plural: str, n: int) -> str:
    """Plural-aware translation for the current locale."""
    return _current.get().ngettext(singular, plural, n)


F = TypeVar("F", bound=Callable[..., Awaitable[Any]])


def localized(handler: F) -> F:
    """Decorator for gRPC handlers: set the request's locale from `req.ses.language` before running."""
    @wraps(handler)
    async def wrapper(self: Any, req: Any, *args: Any, **kwargs: Any) -> Any:
        set_locale(getattr(getattr(req, "ses", None), "language", None))
        return await handler(self, req, *args, **kwargs)
    return wrapper  # type: ignore[return-value]
