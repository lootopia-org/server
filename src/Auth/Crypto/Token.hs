module Auth.Crypto.Token
  ( randomToken
  , randomBytes
  ) where

import           Crypto.Random          (getRandomBytes)
import           Data.ByteArray.Encoding (Base (Base64URLUnpadded), convertToBase)
import qualified Data.ByteString        as BS
import           Data.Text              (Text)
import           Data.Text.Encoding     (decodeUtf8)

randomBytes :: Int -> IO BS.ByteString
randomBytes = getRandomBytes

randomToken :: Int -> IO Text
randomToken n = do
  bs <- randomBytes n
  pure $ decodeUtf8 (convertToBase Base64URLUnpadded bs)
