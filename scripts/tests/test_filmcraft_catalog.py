import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec=importlib.util.spec_from_file_location('audit_filmcraft_catalog',Path(__file__).resolve().parents[1]/'audit-filmcraft-catalog.py')
audit=importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)

class CatalogTests(unittest.TestCase):
 def setUp(self):
  self.directory=tempfile.TemporaryDirectory();self.addCleanup(self.directory.cleanup);self.reference=Path(self.directory.name)
  files={'crates/automation/src/server.rs':'#[tool(title="Query", annotations(read_only_hint=true))]\nasync fn inspect() {}\npub struct Params {}\n#[cfg(test)]\npub fn hidden_test() {}', 'crates/example/Cargo.toml':'[package]\nname="example"\n[features]\ndefault=[]\ngpu=[]\n','docs/contract.md':'# Contract\n## Required behavior\n'}
  for name,text in files.items():
   path=self.reference/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_text(text)
  def git(_reference,*args):
   if args[0]=='rev-parse':return (audit.PIN+'\n').encode()
   if args[0]=='status':return b''
   if args[0]=='ls-files':return ('\0'.join(files)+'\0').encode()
   raise AssertionError(args)
  self.git=git
  self.runtime={'schemaVersion':1,'sourceCommit':audit.PIN,'commands':[{'id':'edit.insert','label':'Insert'}],'effects':[{'id':'grade','name':'Grade','kind':'Video','params':[{'id':'gain','label':'Gain'}]}],'exportFormats':['h264'],'exportPresets':[{'name':'Web'}]}
 def generate(self,overrides=None):
  with patch.object(audit,'git',side_effect=self.git):return audit.generate(self.reference,self.runtime,overrides or {})
 def test_presence_never_marks_behavior_verified_and_includes_dynamic_metadata(self):
  result=self.generate();self.assertEqual(result['summary']['statuses'],{'unreviewed':9})
  ids={r['id'] for r in result['requirements']}
  self.assertIn('mcp-tool:inspect',ids);self.assertIn('parameter:grade.gain',ids);self.assertIn('build-feature:example.gpu',ids)
  self.assertEqual(result['sourceIndex'][0]['publicDeclarations'][0]['name'],'Params')
  self.assertEqual(len(result['sourceIndex'][0]['publicDeclarations']),1)
 def test_duplicate_catalog_and_unknown_override_fail(self):
  self.runtime['commands'].append(self.runtime['commands'][0])
  with self.assertRaisesRegex(ValueError,'duplicate requirement'):self.generate()
  self.runtime['commands'].pop()
  with self.assertRaisesRegex(ValueError,'absent source'):self.generate({'command:unknown':{'status':'partial'}})
 def test_verified_requires_contract_and_real_qualification_artifact(self):
  with self.assertRaisesRegex(ValueError,'without contract'):self.generate({'command:edit.insert':{'status':'verified','evidence':[{'path':'docs/filmcraft-parity.md'}]}})
  override={'status':'verified','evidence':[{'path':'docs/filmcraft-parity.md'}],'qualification':{'sourceContract':'insert semantics','verificationCommand':'test','scope':'insert','resultArtifact':'does-not-exist.json'}}
  with self.assertRaisesRegex(ValueError,'missing qualification artifact'):self.generate({'command:edit.insert':override})
 def test_wrong_snapshot_dirty_reference_and_absent_host_evidence_fail(self):
  self.runtime['sourceCommit']='wrong'
  with self.assertRaisesRegex(ValueError,'provenance'):self.generate()
  self.runtime['sourceCommit']=audit.PIN
  with patch.object(audit,'git',return_value=b'wrong'):
   with self.assertRaisesRegex(ValueError,'HEAD differs'):audit.generate(self.reference,self.runtime,{})
  with patch.object(audit,'git',side_effect=lambda reference,*args:(audit.PIN+'\n').encode() if args[0]=='rev-parse' else b'M tracked.rs'):
   with self.assertRaisesRegex(ValueError,'tracked changes'):audit.generate(self.reference,self.runtime,{})
  with self.assertRaisesRegex(ValueError,'missing host evidence'):self.generate({'command:edit.insert':{'status':'partial','evidence':[{'path':'does-not-exist.rs'}]}})

if __name__=='__main__':unittest.main()
