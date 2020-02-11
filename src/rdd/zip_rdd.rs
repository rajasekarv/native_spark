use std::cmp::min;
use std::marker::PhantomData;
use std::net::Ipv4Addr;
use std::sync::{Arc};

use serde_derive::{Deserialize, Serialize};
use crate::serializable_traits::{AnyData, Data, Func, SerFunc};

use crate::context::Context;
use crate::dependency::{Dependency, OneToOneDependency};
use crate::error::{Error, Result};
use crate::partitioner::{HashPartitioner, Partitioner};
use crate::rdd::{Rdd, RddBase, RddVals};
use crate::split::Split;

#[derive(Clone, Serialize, Deserialize)]
pub struct ZippedPartitionsSplit {
    fst_idx: usize,
    sec_idx: usize,
    idx: usize,

    #[serde(with = "serde_traitobject")]
    fst_split: Box<dyn Split>,
    #[serde(with = "serde_traitobject")]
    sec_split: Box<dyn Split>,
}

impl Split for ZippedPartitionsSplit {
    fn get_index(&self) -> usize {
        self.idx
    }
}

#[derive(Serialize, Deserialize)]
pub struct ZippedPartitionsRdd<F: Data, S: Data> {
    #[serde(with = "serde_traitobject")]
    first: Arc<dyn Rdd<Item = F>>,
    #[serde(with = "serde_traitobject")]
    second: Arc<dyn Rdd<Item = S>>,
    vals: Arc<RddVals>,
    _marker_t: PhantomData<(F, S)>,
}

impl<F: Data, S: Data> Clone for ZippedPartitionsRdd<F, S> {
    fn clone(&self) -> Self {
        ZippedPartitionsRdd {
            first: self.first.clone(),
            second: self.second.clone(),
            vals: self.vals.clone(),
            _marker_t: PhantomData
        }
    }
}

impl<F: Data, S: Data> RddBase for ZippedPartitionsRdd<F, S> {
    fn get_rdd_id(&self) -> usize {
        self.vals.id
    }

    fn get_context(&self) -> Arc<Context> {
        self.vals.context.clone()
    }

    fn get_dependencies(&self) -> Vec<Dependency> {
        self.vals.dependencies.clone()
    }

    fn splits(&self) -> Vec<Box<dyn Split>> {
        let mut arr = Vec::with_capacity(
            min(self.first.number_of_splits(), self.second.number_of_splits())
        );

        for (fst, sec) in self.first.splits().iter().zip(self.second.splits().iter()) {
            let fst_idx = fst.get_index();
            let sec_idx = sec.get_index();

            arr.push(
                Box::new(
                    ZippedPartitionsSplit {
                        fst_idx,
                        sec_idx,
                        idx: fst_idx,
                        fst_split: fst.clone(),
                        sec_split: sec.clone()
                    }
                ) as Box<dyn Split>
            )
        }
        arr
    }

    fn number_of_splits(&self) -> usize {
        self.splits().len()
    }

    fn iterator_any(
        &self,
        split: Box<dyn Split>,
    ) -> Result<Box<dyn Iterator<Item = Box<dyn AnyData>>>> {
        Ok(Box::new(
            self.iterator(split)?
                .map(|x| Box::new(x) as Box<dyn AnyData>),
        ))
    }

    fn cogroup_iterator_any(
        &self,
        split: Box<dyn Split>,
    ) -> Result<Box<dyn Iterator<Item = Box<dyn AnyData>>>> {
        self.iterator_any(split)
    }
}

impl<F: Data, S: Data> Rdd for ZippedPartitionsRdd<F, S> {
    type Item = (F, S);

    fn get_rdd(&self) -> Arc<dyn Rdd<Item = Self::Item>> {
        Arc::new(self.clone())
    }

    fn get_rdd_base(&self) -> Arc<dyn RddBase> {
        Arc::new(self.clone()) as Arc<dyn RddBase>
    }

    fn compute(&self, split: Box<dyn Split>) -> Result<Box<dyn Iterator<Item = Self::Item>>> {
        let current_split = split
            .downcast::<ZippedPartitionsSplit>()
            .or(Err(Error::SplitDowncast("ZippedPartitionsSplit")))?;

        let fst_iter = self.first.iterator(current_split.fst_split.clone())?;
        let sec_iter = self.second.iterator(current_split.sec_split.clone())?;
        Ok(
            Box::new(
                fst_iter.zip(sec_iter)
            )
        )
    }

    fn iterator(&self, split: Box<dyn Split>) -> Result<Box<dyn Iterator<Item = Self::Item>>> {
        self.compute(split.clone())
    }
}

impl<F: Data, S: Data> ZippedPartitionsRdd<F, S> {
    pub fn new(
        first: Arc<dyn Rdd<Item = F>>,
        second: Arc<dyn Rdd<Item = S>>
    ) -> Self {
        let mut vals = RddVals::new(first.get_context());
        vals.dependencies
            .push(Dependency::NarrowDependency(Arc::new(
                OneToOneDependency::new(first.get_rdd_base()),
            )));
        let vals = Arc::new(vals);

        ZippedPartitionsRdd {
            first,
            second,
            vals,
            _marker_t: PhantomData
        }
    }
}