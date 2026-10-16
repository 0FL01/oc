"""Bounded actual prompt acceptance; all responses are local ordinary text SSE."""
from prompt_caret_fixture import emit_message

FIRST = 'HISTORY-FIRST Ω界'
SECOND = 'HISTORY-SECOND\nsecond line Ω界'
THIRD = 'HISTORY-THIRD @note.txt done'
INPUTS = [FIRST, SECOND, THIRD, THIRD]
main_requests = 0
title_requests = 0

def respond(handler, body, spec, emit, title, number):
    global main_requests, title_requests
    users = []
    for item in body.get('input', []):
        if item.get('role') != 'user':
            continue
        content = item.get('content', [])
        users.append(content if isinstance(content, str) else ''.join(
            part.get('text', '') for part in content if part.get('type') in ('input_text', 'text')))
    if title:
        title_requests += 1
        text = 'VIS history one' if title_requests == 1 else 'VIS history two'
        valid = title_requests <= 2
    else:
        main_requests += 1
        valid = main_requests <= len(INPUTS) and users and users[-1] == INPUTS[main_requests-1]
        valid = valid and (main_requests != 4 or users == [THIRD])
        valid = valid and 'HISTORY_FILE_CURRENT_CANARY' not in str(body)
        text = f'VIS-HISTORY-DONE-{main_requests}: exact user bytes accepted.'
    valid = bool(valid and handler.path == '/v1/responses' and body.get('stream') is True
                 and body.get('model') == 'fixture-model-1' and number <= 6)
    emit({'kind':'provider','operation':'prompt_history_title' if title else 'prompt_history_submit',
          'valid':valid,'index':main_requests,'user_texts':users})
    if not valid:
        handler.send_error(400, 'VIS12 exact history input rejected')
        return
    emit_message(handler, body, text, number)
    emit({'kind':'provider_completed','operation':'prompt_history_title' if title else 'prompt_history_submit'})
