WITH logs AS (
            SELECT t.result,t.archive_rowid,t.prompt,m.id,
              COALESCE((SELECT b.value->>'$.id' FROM json_each(?4) b,json_each(b.value,'$.members') member WHERE member.value=m.id LIMIT 1),
                (SELECT cm.block_id FROM compression_members cm WHERE cm.message_id=m.id AND cm.block_id IN (SELECT value->>'$.id' FROM json_each(?4)) LIMIT 1)) AS block_id
            FROM conversation_turns t JOIN conversation_messages m ON m.id=json_extract(t.result,'$.user_message')
            WHERE t.session_id=?1 AND json_valid(t.result) AND m.seq>?2
              AND (m.id IN(SELECT value FROM json_each(?3)) OR EXISTS(SELECT 1 FROM compression_members cm JOIN json_each(?4) b ON b.value->>'$.id'=cm.block_id WHERE cm.message_id=m.id AND COALESCE(b.value->>'$.hot.standalone',0)=0 AND COALESCE(b.value->>'$.hot.active',1)=1))
              AND NOT EXISTS(SELECT 1 FROM json_each(?4) b JOIN messages a ON a.id=b.value->>'$.start' JOIN messages z ON z.id=b.value->>'$.end' WHERE b.value->>'$.hot.standalone'=1 AND b.value->>'$.hot.active'=1 AND m.seq BETWEEN a.seq AND z.seq)
          ), visible AS (
            SELECT json_object('turn_id',result->>'$.turn_id','model',result->>'$.model','provider',result->>'$.provider',
                'protocol',COALESCE(result->>'$.protocol','responses'),'binding',json(result->'$.binding'),
               'agent_digest',result->>'$.agent_digest','user_message',id,'assistant_message',result->>'$.assistant_message',
               'raw_prefix',json(result->'$.raw_prefix'),'working',json(result->'$.working'),
                'requests',json(COALESCE(result->>'$.requests','[]')),
                'spans',json(COALESCE(result->>'$.spans','[]')),
               'call_occurrences',json(COALESCE((SELECT json_group_object(
                 (SELECT count(*) FROM json_each(logs.result,'$.input') i WHERE i.key<CAST(c.key AS INTEGER)
                   AND (logs.id IN (SELECT value FROM json_each(?3)) OR NOT (i.value->>'$.type'='message' AND i.value->>'$.id' IS NULL AND i.value->>'$.phase' IS NULL AND i.value->>'$.status' IS NULL))
                   AND (i.value->>'$.type'!='function_call' OR EXISTS(SELECT 1 FROM json_each(logs.result,'$.input') answered WHERE answered.value->>'$.type'='function_call_output' AND answered.value->>'$.call_id'=i.value->>'$.call_id'))),c.value)
                  FROM json_each(logs.result,'$.call_occurrences') c WHERE EXISTS(SELECT 1 FROM json_each(logs.result,'$.input') original WHERE original.key=CAST(c.key AS INTEGER) AND original.value->>'$.type'='function_call' AND EXISTS(SELECT 1 FROM json_each(logs.result,'$.input') answered WHERE answered.value->>'$.type'='function_call_output' AND answered.value->>'$.call_id'=original.value->>'$.call_id'))),'{}')),
               'input_origins',json(COALESCE((SELECT json_group_array(i.key+COALESCE(result->>'$.raw_prefix.ends[0]',0)) FROM json_each(result,'$.input') i
                 WHERE (logs.id IN (SELECT value FROM json_each(?3)) OR NOT (i.value->>'$.type'='message' AND i.value->>'$.id' IS NULL AND i.value->>'$.phase' IS NULL AND i.value->>'$.status' IS NULL))
                 AND (i.value->>'$.type'!='function_call' OR EXISTS(SELECT 1 FROM json_each(logs.result,'$.input') answered WHERE answered.value->>'$.type'='function_call_output' AND answered.value->>'$.call_id'=i.value->>'$.call_id'))),'[]')),
               'shell_notice_messages',json(CASE WHEN logs.id IN (SELECT value FROM json_each(?3)) THEN COALESCE(result->>'$.shell_notice_messages','[]') ELSE '[]' END),
               'native_input',json_extract(result,'$.native_input'),
               'instruction_references',json(COALESCE((SELECT json_group_array(json_object(
                 'event',a.value->>'$.event',
                 'index',(SELECT count(*) FROM json_each(logs.result,'$.input') i WHERE i.key<a.value->>'$.index'
                   AND (logs.id IN (SELECT value FROM json_each(?3)) OR NOT (i.value->>'$.type'='message' AND i.value->>'$.id' IS NULL AND i.value->>'$.phase' IS NULL AND i.value->>'$.status' IS NULL))
                   AND (i.value->>'$.type'!='function_call' OR EXISTS(SELECT 1 FROM json_each(logs.result,'$.input') answered WHERE answered.value->>'$.type'='function_call_output' AND answered.value->>'$.call_id'=i.value->>'$.call_id')))))
                 FROM json_each(logs.result,'$.instruction_references') a),'[]')),
              '_dcp_block',CASE WHEN id IN (SELECT value FROM json_each(?3)) THEN NULL ELSE block_id END,
              '_dcp_prompt',CASE WHEN id IN (SELECT value FROM json_each(?3)) THEN prompt END,
              'display_parts',json(COALESCE((SELECT json_group_array(json(p.value)) FROM json_each(result,'$.display_parts') p WHERE p.value->>'$.tool' IS NOT NULL),'[]')),
              'input',json(COALESCE((SELECT json_group_array(json(i.value)) FROM json_each(result,'$.input') i
                WHERE (logs.id IN (SELECT value FROM json_each(?3)) OR NOT (i.value->>'$.type'='message' AND i.value->>'$.id' IS NULL AND i.value->>'$.phase' IS NULL AND i.value->>'$.status' IS NULL))
                  AND (i.value->>'$.type'!='function_call' OR EXISTS(SELECT 1 FROM json_each(logs.result,'$.input') answered WHERE answered.value->>'$.type'='function_call_output' AND answered.value->>'$.call_id'=i.value->>'$.call_id'))),'[]')),
              'native_mcp_results',json(COALESCE((SELECT json_group_array(json_object(
                'input_index',(SELECT count(*) FROM json_each(logs.result,'$.input') i WHERE i.key<a.value->>'$.input_index'
                  AND (logs.id IN (SELECT value FROM json_each(?3)) OR NOT (i.value->>'$.type'='message' AND i.value->>'$.id' IS NULL AND i.value->>'$.phase' IS NULL AND i.value->>'$.status' IS NULL))
                  AND (i.value->>'$.type'!='function_call' OR EXISTS(SELECT 1 FROM json_each(logs.result,'$.input') answered WHERE answered.value->>'$.type'='function_call_output' AND answered.value->>'$.call_id'=i.value->>'$.call_id'))),
                'call_id',a.value->>'$.call_id','result',json(a.value->'$.result')))
                 FROM json_each(logs.result,'$.native_mcp_results') a),'[]')),
               'native_read_results',json(COALESCE((SELECT json_group_array(json_object(
                 'input_index',(SELECT count(*) FROM json_each(logs.result,'$.input') i WHERE i.key<a.value->>'$.input_index'
                   AND (logs.id IN (SELECT value FROM json_each(?3)) OR NOT (i.value->>'$.type'='message' AND i.value->>'$.id' IS NULL AND i.value->>'$.phase' IS NULL AND i.value->>'$.status' IS NULL))
                   AND (i.value->>'$.type'!='function_call' OR EXISTS(SELECT 1 FROM json_each(logs.result,'$.input') answered WHERE answered.value->>'$.type'='function_call_output' AND answered.value->>'$.call_id'=i.value->>'$.call_id'))),
                 'call_id',a.value->>'$.call_id','result',json(a.value->'$.result')))
                 FROM json_each(logs.result,'$.native_read_results') a),'[]')),
               'native_question_results',json(COALESCE((SELECT json_group_array(json_object(
                 'input_index',(SELECT count(*) FROM json_each(logs.result,'$.input') i WHERE i.key<a.value->>'$.input_index'
                   AND (logs.id IN (SELECT value FROM json_each(?3)) OR NOT (i.value->>'$.type'='message' AND i.value->>'$.id' IS NULL AND i.value->>'$.phase' IS NULL AND i.value->>'$.status' IS NULL))
                   AND (i.value->>'$.type'!='function_call' OR EXISTS(SELECT 1 FROM json_each(logs.result,'$.input') answered WHERE answered.value->>'$.type'='function_call_output' AND answered.value->>'$.call_id'=i.value->>'$.call_id'))),
                 'call_id',a.value->>'$.call_id','result',json(a.value->'$.result')))
                 FROM json_each(logs.result,'$.native_question_results') a),'[]')))
              AS result,archive_rowid
            FROM logs WHERE id IN (SELECT value FROM json_each(?3)) OR block_id IS NOT NULL
          ) SELECT CASE WHEN length(CAST(result AS BLOB))<=?5 THEN result END FROM visible WHERE result->>'$._dcp_block' IS NULL OR json_array_length(result,'$.input')>0 ORDER BY archive_rowid LIMIT 4097