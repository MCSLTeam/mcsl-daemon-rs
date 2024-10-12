use std::collections::BTreeMap;

/// 用于记录上传文件数据的整数区间，支持在区间内减去子区间。
pub struct U64Remain {
    remains: BTreeMap<u64, u64>,
}

impl U64Remain {
    /// 创建新的 LongRemain 实例，定义整数区间 [begin, end)
    pub fn new(begin: u64, end: u64) -> Self {
        let mut remains = BTreeMap::new();
        remains.insert(begin, end);
        Self { remains }
    }

    /// 减去 [from, to) 区间
    pub fn reduce(&mut self, from: u64, to: u64) {
        let mut to_remove = vec![];
        for (&begin, &end) in self.remains.range(..) {
            if from <= begin && to >= end {
                // 完全覆盖
                to_remove.push(begin);
            } else if from > begin && to < end {
                // 中间切割
                self.remains.insert(to, end);
                self.remains.insert(begin, from);
                break;
            } else if begin < from && from < end {
                // 剪切结束
                self.remains.insert(begin, from);
                break;
            } else if begin < to && to < end {
                // 剪切开始
                self.remains.insert(to, end);
                self.remains.remove(&begin);
                break;
            }
        }

        // 删除标记的区间
        for key in to_remove {
            self.remains.remove(&key);
        }
    }

    /// 获取剩余区间
    #[allow(dead_code)]
    pub fn get_remains(&self) -> impl Iterator<Item = (u64, u64)> + '_ {
        self.remains.iter().map(|(&begin, &end)| (begin, end))
    }

    /// 获取剩余区间的总长度
    pub fn get_remain(&self) -> u64 {
        self.remains.iter().map(|(&begin, &end)| end - begin).sum()
    }

    /// 判断是否完成
    #[allow(dead_code)]
    pub fn done(&self) -> bool {
        self.remains.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_u64_remain() {
        let mut remains = U64Remain::new(0, 100);
        assert_eq!(remains.get_remains().collect::<Vec<_>>(), [(0, 100)]);
        assert_eq!(remains.get_remain(), 100);

        remains.reduce(50, 70);
        assert_eq!(
            remains.get_remains().collect::<Vec<_>>(),
            [(0, 50), (70, 100)]
        );
        assert_eq!(remains.get_remain(), 80);

        remains.reduce(30, 40);
        assert_eq!(
            remains.get_remains().collect::<Vec<_>>(),
            [(0, 30), (40, 50), (70, 100)]
        );
        assert_eq!(remains.get_remain(), 70);

        remains.reduce(0, 30);
        assert_eq!(
            remains.get_remains().collect::<Vec<_>>(),
            [(40, 50), (70, 100)]
        );
        assert_eq!(remains.get_remain(), 40);
        assert!(!remains.done());

        remains.reduce(0, 100);
        assert_eq!(remains.get_remains().collect::<Vec<_>>(), []);
        assert_eq!(remains.get_remain(), 0);
        assert!(remains.done())
    }
}
