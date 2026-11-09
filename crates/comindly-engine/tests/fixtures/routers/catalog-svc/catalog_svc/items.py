from web_kit.routing import ScopedRouter

router = ScopedRouter(prefix="/agent", scope="items")


@router.get("/items")
def list_items():
    return []
