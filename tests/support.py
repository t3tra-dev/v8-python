import logging
import unittest
from collections.abc import Callable, Mapping
from typing import Literal

import v8

WebAssemblyLoader = Mapping[str, bytes] | Callable[[str], bytes]
ModuleResolver = (
    Mapping[str, str | v8.Module]
    | Callable[[str, str | None, dict[str, str]], str | v8.Module | None]
)
ImportMetaResolver = Mapping[str, object] | Callable[[str], Mapping[str, object] | None]


class RecordingHandler(logging.Handler):
    def __init__(self):
        super().__init__()
        self.records: list[logging.LogRecord] = []

    def emit(self, record: logging.LogRecord) -> None:
        self.records.append(record)


class V8TestCase(unittest.TestCase):
    def make_context(self) -> v8.Context:
        isolate = v8.Isolate()
        return isolate.create_context()

    def make_timer_context(self) -> v8.Context:
        profile = v8.BaseProfile().install([v8.api.Timer()])
        isolate = v8.Isolate()
        builder = isolate.create_context_builder()
        builder.use_profile(profile)
        return builder.build()

    def make_microtask_context(self) -> v8.Context:
        profile = v8.BaseProfile().install([v8.api.MicrotaskQueue()])
        isolate = v8.Isolate()
        builder = isolate.create_context_builder()
        builder.use_profile(profile)
        return builder.build()

    def make_console_context(self, logger: logging.Logger | None = None) -> v8.Context:
        profile = v8.BaseProfile().install([v8.api.Console(logger)])
        isolate = v8.Isolate()
        builder = isolate.create_context_builder()
        builder.use_profile(profile)
        return builder.build()

    def make_dynamic_code_context(self, allow_eval: bool = False) -> v8.Context:
        profile = v8.BaseProfile().install([v8.api.DynamicCodePolicy(allow_eval)])
        isolate = v8.Isolate()
        builder = isolate.create_context_builder()
        builder.use_profile(profile)
        return builder.build()

    def make_shared_memory_context(self, *, allow_wait: bool = True) -> v8.Context:
        profile = v8.BaseProfile().install(
            [
                v8.api.SharedArrayBuffer(),
                v8.api.Atomics(allow_wait=allow_wait),
            ]
        )
        isolate = v8.Isolate()
        builder = isolate.create_context_builder()
        builder.use_profile(profile)
        return builder.build()

    def make_webassembly_context(
        self,
        loader: WebAssemblyLoader | None = None,
        *,
        allow_code_generation: bool = True,
        cache: v8.WasmModuleCache | None = None,
    ) -> v8.Context:
        profile = v8.BaseProfile().install(
            [
                v8.api.WebAssembly(
                    loader,
                    allow_code_generation=allow_code_generation,
                    cache=cache,
                )
            ]
        )
        isolate = v8.Isolate()
        builder = isolate.create_context_builder()
        builder.use_profile(profile)
        return builder.build()

    def make_inspector_context(self, name: str = "test") -> v8.Context:
        profile = v8.BaseProfile().install([v8.api.Inspector(name)])
        isolate = v8.Isolate()
        builder = isolate.create_context_builder()
        builder.use_profile(profile)
        return builder.build()

    def make_module_loader_context(
        self,
        resolver: ModuleResolver | None,
        *,
        import_meta: ImportMetaResolver | None = None,
    ) -> v8.Context:
        profile = v8.BaseProfile().install(
            [v8.api.ModuleLoader(resolver, import_meta=import_meta)]
        )
        isolate = v8.Isolate()
        builder = isolate.create_context_builder()
        builder.use_profile(profile)
        return builder.build()

    def make_recording_logger(
        self, name: str
    ) -> tuple[logging.Logger, RecordingHandler]:
        logger = logging.getLogger(name)
        handler = RecordingHandler()
        old_handlers = list(logger.handlers)
        old_level = logger.level
        old_propagate = logger.propagate
        logger.handlers[:] = [handler]
        logger.setLevel(logging.DEBUG)
        logger.propagate = False

        def restore():
            logger.handlers[:] = old_handlers
            logger.setLevel(old_level)
            logger.propagate = old_propagate

        self.addCleanup(restore)
        return logger, handler

    def make_promise_tracker_context(
        self,
        *,
        policy: Literal["ignore", "warn"] = "warn",
        callback: Callable[[str, str | None], object] | None = None,
    ) -> v8.Context:
        profile = v8.BaseProfile().install(
            [v8.api.PromiseRejectionTracker(policy=policy, callback=callback)]
        )
        isolate = v8.Isolate()
        builder = isolate.create_context_builder()
        builder.use_profile(profile)
        return builder.build()
