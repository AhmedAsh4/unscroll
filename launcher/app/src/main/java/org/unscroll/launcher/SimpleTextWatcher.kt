package org.unscroll.launcher

import android.text.Editable
import android.text.TextWatcher

class SimpleTextWatcher(private val changed: (String) -> Unit) : TextWatcher {
    override fun beforeTextChanged(text: CharSequence?, start: Int, count: Int, after: Int) = Unit
    override fun onTextChanged(text: CharSequence?, start: Int, before: Int, count: Int) = changed(text?.toString().orEmpty())
    override fun afterTextChanged(text: Editable?) = Unit
}
