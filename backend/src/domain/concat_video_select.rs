//! Bounded-depth FFmpeg ordinal selection; caller supplies sorted unique indices.
use anyhow::{ensure,Result};
pub fn expression(ordinals:&[usize])->Result<String>{
 ensure!(!ordinals.is_empty()&&ordinals.len()<=360000&&ordinals.windows(2).all(|pair|pair[0]<pair[1]),"invalid video selection ordinals");
 let mut ranges=Vec::new();let mut start=ordinals[0];let mut end=start;
 for &ordinal in &ordinals[1..]{if end.checked_add(1)==Some(ordinal){end=ordinal}else{ranges.push((start,end));start=ordinal;end=ordinal;}}
 ranges.push((start,end));let mut output=String::with_capacity(ranges.len()*32);
 fn write(ranges:&[(usize,usize)],output:&mut String){use std::fmt::Write;if let [(start,end)]=ranges{if start==end{write!(output,"eq(n\\,{start})").unwrap()}else{write!(output,"between(n\\,{start}\\,{end})").unwrap()}}else{let middle=ranges.len()/2;output.push('(');write(&ranges[..middle],output);output.push('+');write(&ranges[middle..],output);output.push(')');}}
 write(&ranges,&mut output);Ok(output)
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn compresses_contiguous_ranges_without_losing_gaps(){assert_eq!(expression(&[0,1,2,4,6,7]).unwrap(),"(between(n\\,0\\,2)+(eq(n\\,4)+between(n\\,6\\,7)))");assert_eq!(expression(&[usize::MAX-1,usize::MAX]).unwrap(),format!("between(n\\,{}\\,{})",usize::MAX-1,usize::MAX));}
 #[test]fn sparse_full_budget_has_logarithmic_binary_depth(){let ordinals:Vec<_>=(0..360000).map(|n|n*2).collect();let value=expression(&ordinals).unwrap();let mut depth=0;let mut maximum=0;for byte in value.bytes(){if byte==b'('{depth+=1;maximum=maximum.max(depth)}else if byte==b')'{depth-=1}}assert_eq!(depth,0);assert!(maximum<=20,"depth {maximum}");assert_eq!(value.matches("eq(").count(),360000);}
 #[test]fn refuses_empty_duplicates_and_reversed_indices(){for values in [&[][..],&[0,0],&[2,1]]{assert!(expression(values).is_err());}}
}
