"""Small CPU-only regression tests for the training evidence boundary."""
from pathlib import Path
import json,tempfile,unittest
from supervision_contract import REVISION,feature_digest,validate_supervision
class SupervisionTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.root=Path(self.temp.name);self.rows=[]
        for i in range(16):
            p=self.root/f'{i}.json';p.write_text(json.dumps({'model_revision':REVISION,'image_tokens':1,'features':[i*0.01]*1024}),encoding='utf-8')
            self.rows.append({'features':str(p),'group':f'context-{i}','confirmed':True,'label':'B','split':'train' if i<8 else 'validation'})
    def tearDown(self):self.temp.cleanup()
    def test_content_disjoint_split_is_accepted(self):
        result=validate_supervision(self.rows);self.assertEqual(result['unique_features'],16)
    def test_same_tensor_with_another_filename_and_group_cannot_leak(self):
        self.rows[8]['features']=self.rows[0]['features']
        with self.assertRaisesRegex(ValueError,'Identical visual content'):validate_supervision(self.rows)
    def test_same_context_cannot_cross_splits(self):
        self.rows[8]['group']=self.rows[0]['group']
        with self.assertRaisesRegex(ValueError,'group leaks'):validate_supervision(self.rows)
    def test_self_labelled_sample_is_rejected(self):
        self.rows[3]['confirmed']=False
        with self.assertRaisesRegex(ValueError,'explicit confirmed'):validate_supervision(self.rows)
    def test_nonfinite_features_are_rejected(self):
        x={'model_revision':REVISION,'image_tokens':1,'features':[float('nan')]*1024}
        with self.assertRaisesRegex(ValueError,'feature values'):feature_digest(x)
    def test_same_split_duplicates_do_not_increase_evidence(self):
        self.rows[1]['features']=self.rows[0]['features']
        with self.assertRaisesRegex(ValueError,'Duplicate visual example'):validate_supervision(self.rows)
if __name__=='__main__':unittest.main()
