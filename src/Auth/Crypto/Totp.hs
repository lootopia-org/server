module Auth.Crypto.Totp
  ( generateSecret
  , secretToBase32
  , otpauthUri
  , verifyCode
  ) where

import           Crypto.Hash.Algorithms (SHA1 (..))
import           Crypto.MAC.HMAC        (HMAC, hmac)
import           Crypto.Random          (getRandomBytes)
import           Data.Bits              (shiftL, shiftR, (.&.), (.|.))
import           Data.ByteArray         (convert)
import qualified Data.ByteString        as BS
import           Data.Text              (Text)
import qualified Data.Text              as T
import           Data.Time.Clock.POSIX  (getPOSIXTime)
import           Data.Word              (Word32, Word64)
import           Text.Printf            (printf)

import qualified Auth.Crypto.Base32     as B32

period :: Word64
period = 30

digits :: Int
digits = 6

generateSecret :: IO BS.ByteString
generateSecret = getRandomBytes 20

secretToBase32 :: BS.ByteString -> Text
secretToBase32 = T.pack . B32.encode

otpauthUri :: Text -> Text -> BS.ByteString -> Text
otpauthUri issuer account secret =
  "otpauth://totp/" <> enc issuer <> ":" <> enc account
    <> "?secret=" <> secretToBase32 secret
    <> "&issuer=" <> enc issuer
    <> "&algorithm=SHA1&digits=" <> T.pack (show digits)
    <> "&period=" <> T.pack (show period)
  where
    enc = T.concatMap escape
    escape ' ' = "%20"
    escape '/' = "%2F"
    escape ':' = "%3A"
    escape '?' = "%3F"
    escape '&' = "%26"
    escape '=' = "%3D"
    escape c   = T.singleton c

counterBytes :: Word64 -> BS.ByteString
counterBytes c = BS.pack [ fromIntegral (c `shiftR` (8 * (7 - i)) .&. 0xff) | i <- [0 .. 7] ]

hotp :: BS.ByteString -> Word64 -> String
hotp secret counter =
  let mac    = convert (hmac secret (counterBytes counter) :: HMAC SHA1) :: BS.ByteString
      offset = fromIntegral (BS.index mac 19 .&. 0x0f)
      at i   = fromIntegral (BS.index mac (offset + i)) :: Word32
      binCode = ((at 0 .&. 0x7f) `shiftL` 24)
            .|. ((at 1 .&. 0xff) `shiftL` 16)
            .|. ((at 2 .&. 0xff) `shiftL` 8)
            .|.  (at 3 .&. 0xff)
      value  = binCode `mod` (10 ^ digits)
  in printf "%0*u" digits value

verifyCode :: BS.ByteString -> Text -> IO Bool
verifyCode secret code = do
  now <- getPOSIXTime
  let counter   = floor (realToFrac now / fromIntegral period :: Double) :: Word64
      candidate = T.unpack (T.strip code)
      valid     = [ hotp secret c | c <- [counter - 1, counter, counter + 1] ]
  pure (candidate `elem` valid)
