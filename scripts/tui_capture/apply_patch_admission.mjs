// Fixture-only admission. The bundled U19 executor remains registered and unchanged.
import fs from 'node:fs';
import path from 'node:path';
export default {id:'vis35.fixture.patch-admission',async setup(ctx) {
  const log=value=>fs.appendFileSync(path.join(process.env.HOME,'vis35-hook.jsonl'),JSON.stringify(value)+'\n');
  const registrations=[];
  registrations.push(await ctx.session.hook('context',event=>{
    if(event.model.providerID!=='fixture'||event.model.id!=='fixture-model-1')return;
    // U19 Input: one required string. This only admits the existing Core tool.
    event.tools.patch={description:'Apply a file patch',input:{type:'object',properties:{patchText:{type:'string',description:'The full patch text describing add, update, and delete operations'}},required:['patchText'],additionalProperties:false}};
    delete event.tools.edit;delete event.tools.write;
    log({kind:'admission',model:event.model,tools:Object.keys(event.tools),patch:event.tools.patch});
  }));
  registrations.push(await ctx.tool.hook('execute.before',event=>{if(event.tool==='patch')log({kind:'execute.before',...event});}));
  registrations.push(await ctx.tool.hook('execute.after',event=>{if(event.tool==='patch')log({kind:'execute.after',...event});}));
  log({kind:'setup',registered_tools:(await ctx.tool.list()).map(t=>t.name)});
  return async()=>{for(const registration of registrations)await registration.dispose();};
}};
