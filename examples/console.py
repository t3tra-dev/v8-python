import logging

import v8
from _shared import make_profile_context

logging.basicConfig(level=logging.DEBUG, format="%(levelname)s:%(message)s")
logger = logging.getLogger("example.v8.console")

context = make_profile_context([v8.api.Console(logger)])
context.eval(
    """
    console.log("hello", 42);
    console.warn("careful");
    console.count("items");
    console.count("items");
    """
)
