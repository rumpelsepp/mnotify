"""Run a command in a pseudo-terminal and answer its emoji prompt.

    python3 pty_answer.py ANSWER LOG COMMAND...

The output goes to LOG. Once the command asks to confirm the emojis, ANSWER
("y" or "n") and Enter are typed, and the terminal answers the prompt's
cursor position query. Exits with the command's exit code.
"""

import os
import pty
import sys

PROMPT = b"Confirm that the emojis match"
# The prompt asks the terminal where the cursor is before it reads keys.
CURSOR_QUERY = b"\x1b[6n"


def main() -> int:
    answer, log, *command = sys.argv[1:]
    pid, fd = pty.fork()
    if pid == 0:
        os.execvp(command[0], command)

    seen = b""
    answered = False
    with open(log, "wb", buffering=0) as out:
        while True:
            try:
                data = os.read(fd, 4096)
            except OSError:  # EIO: the command closed the terminal
                break
            if not data:
                break
            out.write(data)
            seen += data
            for _ in range(data.count(CURSOR_QUERY)):
                os.write(fd, b"\x1b[1;1R")
            if not answered and PROMPT in seen and CURSOR_QUERY in seen.split(PROMPT)[-1]:
                os.write(fd, answer.encode() + b"\r")
                answered = True
    _, status = os.waitpid(pid, 0)
    return os.waitstatus_to_exitcode(status)


if __name__ == "__main__":
    sys.exit(main())
