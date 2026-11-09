from service_a.jobs import run
from service_a.users import UserService


def main():
    run()
    svc = UserService(None)
    svc.fetch(1)
