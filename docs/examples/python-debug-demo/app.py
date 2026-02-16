import time


def compute(value: int) -> int:
    doubled = value * 2
    result = doubled + 3
    return result


def main() -> None:
    counter = 0
    while counter < 3:
        current = compute(counter)
        print(f"counter={counter} current={current}", flush=True)
        counter += 1
        time.sleep(0.5)

    time.sleep(30)


if __name__ == "__main__":
    main()
