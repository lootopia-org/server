module Auth.Crypto.Base32
  ( encode
  , decode
  ) where

import           Data.Bits             (shiftL, shiftR, (.&.), (.|.))
import qualified Data.ByteString       as BS
import           Data.Char             (toUpper)
import           Data.List             (elemIndex, foldl', unfoldr)
import           Data.Word             (Word8)

alphabet :: String
alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"

encode = concatMap chunkToChars . chunk5 . BS.unpack
  where
    chunk5 = unfoldr step
      where
        step [] = Nothing
        step xs = Just (splitAt 5 xs)

    chunkToChars :: [Word8] -> String
    chunkToChars bytes =
      let n        = length bytes
          value    = foldl' (\acc b -> (acc `shiftL` 8) .|. fromIntegral b) (0 :: Integer) bytes
          padded   = value `shiftL` (8 * (5 - n))
          outChars = case n of
            1 -> 2; 2 -> 4; 3 -> 5; 4 -> 7; _ -> 8
      in [ alphabet !! fromIntegral ((padded `shiftR` (35 - 5 * i)) .&. 0x1f)
         | i <- [0 .. outChars - 1] ]

decode :: String -> Maybe BS.ByteString
decode = fmap BS.pack . go . map toUpper . filter (`notElem` (" \t\r\n=" :: String))
  where
    go [] = Just []
    go s  =
      let (group, rest) = splitAt 8 s
      in do
        idxs <- mapM (`elemIndex` alphabet) group
        let nBytes = (length group * 5) `div` 8
            value  = foldl' (\acc d -> (acc `shiftL` 5) .|. fromIntegral d) (0 :: Integer) idxs
            shifted = value `shiftL` (5 * (8 - length group))
            bytes  = [ fromIntegral ((shifted `shiftR` (32 - 8 * i)) .&. 0xff)
                     | i <- [0 .. nBytes - 1] ]
        more <- go rest
        Just (bytes ++ more)
